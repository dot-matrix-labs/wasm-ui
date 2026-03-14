use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram, WebGlShader,
    WebGlTexture, WebGlUniformLocation, WebGlVertexArrayObject,
};

use ui_core::batch::{Batch, Material, Quad, TextRun};
use ui_core::types::Rect;

use crate::atlas::TextAtlas;

/// Cached uniform locations for a shader program.
struct ProgramUniforms {
    program: WebGlProgram,
    u_resolution: Option<WebGlUniformLocation>,
    u_atlas: Option<WebGlUniformLocation>,
}

impl ProgramUniforms {
    fn new(gl: &Gl, program: WebGlProgram) -> Self {
        let u_resolution = gl.get_uniform_location(&program, "u_resolution");
        let u_atlas = gl.get_uniform_location(&program, "u_atlas");
        Self {
            program,
            u_resolution,
            u_atlas,
        }
    }
}

pub struct Renderer {
    gl: Gl,
    solid_program: ProgramUniforms,
    text_program: ProgramUniforms,
    vbo: WebGlBuffer,
    ibo: WebGlBuffer,
    vao: WebGlVertexArrayObject,
    atlas: TextAtlas,
    atlas_texture: WebGlTexture,
    width: f32,
    height: f32,
}

impl Renderer {
    pub fn new(canvas: &HtmlCanvasElement, width: f32, height: f32) -> Result<Self, JsValue> {
        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("WebGL2 not supported"))?
            .dyn_into()?;

        let solid_prog = link_program(&gl, SOLID_VERT_SHADER, SOLID_FRAG_SHADER)?;
        let text_prog = link_program(&gl, TEXT_VERT_SHADER, TEXT_FRAG_SHADER)?;

        let vbo = gl
            .create_buffer()
            .ok_or_else(|| JsValue::from_str("no vbo"))?;
        let ibo = gl
            .create_buffer()
            .ok_or_else(|| JsValue::from_str("no ibo"))?;
        let vao = gl
            .create_vertex_array()
            .ok_or_else(|| JsValue::from_str("no vao"))?;
        let atlas_texture = gl
            .create_texture()
            .ok_or_else(|| JsValue::from_str("no texture"))?;

        // Set up the VAO with vertex attrib pointers once.
        // Both programs share the same vertex layout, so we use solid_prog for
        // attribute locations (they are identical across programs thanks to
        // the same `layout(location = N)` qualifiers).
        gl.bind_vertex_array(Some(&vao));
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&vbo));
        gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&ibo));

        let stride = 9 * 4; // 9 floats * 4 bytes
        // location 0 = a_pos (vec2)
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(0, 2, Gl::FLOAT, false, stride, 0);
        // location 1 = a_uv (vec2)
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_with_i32(1, 2, Gl::FLOAT, false, stride, 2 * 4);
        // location 2 = a_color (vec4)
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_with_i32(2, 4, Gl::FLOAT, false, stride, 4 * 4);
        // location 3 = a_flags (float)
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_with_i32(3, 1, Gl::FLOAT, false, stride, 8 * 4);

        gl.bind_vertex_array(None);

        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        let solid_program = ProgramUniforms::new(&gl, solid_prog);
        let text_program = ProgramUniforms::new(&gl, text_prog);

        let mut renderer = Self {
            gl,
            solid_program,
            text_program,
            vbo,
            ibo,
            vao,
            atlas: TextAtlas::new(1024, 1024),
            atlas_texture,
            width,
            height,
        };
        renderer.init_atlas_texture();
        renderer.resize(width, height);
        Ok(renderer)
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.gl.viewport(0, 0, width as i32, height as i32);
    }

    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        self.atlas.set_font_bytes(bytes);
    }

    pub fn render(&mut self, batch: &Batch, text_runs: &[TextRun]) -> Result<(), JsValue> {
        let mut merged = batch.clone();
        for run in text_runs {
            self.push_text_quads(&mut merged, run.clone());
        }
        self.upload_atlas_if_needed();
        self.draw_batch(&merged)
    }

    fn push_text_quads(&mut self, batch: &mut Batch, run: TextRun) {
        let mut x = run.rect.x;
        let mut y = run.rect.y + run.rect.h * 0.7;
        let font_size = run.font_size;
        let line_height = font_size * 1.4;
        for ch in run.text.chars() {
            if ch == '\n' {
                x = run.rect.x;
                y += line_height;
                continue;
            }
            let glyph = self.atlas.ensure_glyph(ch, font_size);
            let rect = Rect::new(
                x + glyph.bearing.x,
                y - glyph.size.y + glyph.bearing.y,
                glyph.size.x,
                glyph.size.y,
            );
            batch.push_quad(
                Quad {
                    rect,
                    uv: glyph.uv,
                    color: run.color,
                    flags: 1,
                },
                Material::TextAtlas,
                run.clip,
            );
            x += glyph.advance;
        }
    }

    fn init_atlas_texture(&mut self) {
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas_texture));
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
        let data = self.atlas.pixels();
        let width = 1024;
        let height = 1024;
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            Gl::R8 as i32,
            width,
            height,
            0,
            Gl::RED,
            Gl::UNSIGNED_BYTE,
            Some(data),
        )
        .ok();
        self.atlas.mark_clean();
    }

    fn upload_atlas_if_needed(&mut self) {
        if !self.atlas.is_dirty() {
            return;
        }
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas_texture));
        let data = self.atlas.pixels();
        gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            0,
            0,
            1024,
            1024,
            Gl::RED,
            Gl::UNSIGNED_BYTE,
            Some(data),
        )
        .ok();
        self.atlas.mark_clean();
    }

    fn draw_batch(&mut self, batch: &Batch) -> Result<(), JsValue> {
        let gl = &self.gl;

        // Upload vertex + index data into the VBO/IBO (VAO remembers the bindings).
        let mut vertex_data: Vec<f32> = Vec::with_capacity(batch.vertices.len() * 9);
        for v in &batch.vertices {
            vertex_data.push(v.pos.x);
            vertex_data.push(v.pos.y);
            vertex_data.push(v.uv.x);
            vertex_data.push(v.uv.y);
            vertex_data.push(v.color.r);
            vertex_data.push(v.color.g);
            vertex_data.push(v.color.b);
            vertex_data.push(v.color.a);
            vertex_data.push(v.flags as f32);
        }
        let index_data = batch.indices.clone();

        gl.bind_vertex_array(Some(&self.vao));

        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.vbo));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&vertex_data);
            gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &vert_array, Gl::DYNAMIC_DRAW);
        }
        gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.ibo));
        unsafe {
            let idx_array = js_sys::Uint32Array::view(&index_data);
            gl.buffer_data_with_array_buffer_view(
                Gl::ELEMENT_ARRAY_BUFFER,
                &idx_array,
                Gl::DYNAMIC_DRAW,
            );
        }

        gl.clear_color(0.97, 0.97, 0.96, 1.0);
        gl.clear(Gl::COLOR_BUFFER_BIT);

        let mut current_material: Option<Material> = None;

        for cmd in &batch.commands {
            // Switch program when material changes.
            if current_material != Some(cmd.material) {
                current_material = Some(cmd.material);
                match cmd.material {
                    Material::Solid => {
                        gl.use_program(Some(&self.solid_program.program));
                        if let Some(ref loc) = self.solid_program.u_resolution {
                            gl.uniform2f(Some(loc), self.width, self.height);
                        }
                    }
                    Material::TextAtlas | Material::IconAtlas => {
                        gl.use_program(Some(&self.text_program.program));
                        if let Some(ref loc) = self.text_program.u_resolution {
                            gl.uniform2f(Some(loc), self.width, self.height);
                        }
                        gl.active_texture(Gl::TEXTURE0);
                        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas_texture));
                        if let Some(ref loc) = self.text_program.u_atlas {
                            gl.uniform1i(Some(loc), 0);
                        }
                    }
                }
            }

            if let Some(clip) = cmd.clip {
                gl.enable(Gl::SCISSOR_TEST);
                gl.scissor(
                    clip.x as i32,
                    (self.height - clip.y - clip.h) as i32,
                    clip.w as i32,
                    clip.h as i32,
                );
            } else {
                gl.disable(Gl::SCISSOR_TEST);
            }

            gl.draw_elements_with_i32(
                Gl::TRIANGLES,
                cmd.count as i32,
                Gl::UNSIGNED_INT,
                (cmd.start * 4) as i32,
            );
        }

        gl.bind_vertex_array(None);

        Ok(())
    }
}

fn compile_shader(gl: &Gl, source: &str, shader_type: u32) -> Result<WebGlShader, JsValue> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or_else(|| JsValue::from_str("unable to create shader"))?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    if gl
        .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(JsValue::from_str(
            &gl.get_shader_info_log(&shader).unwrap_or_default(),
        ))
    }
}

fn link_program(gl: &Gl, vert_src: &str, frag_src: &str) -> Result<WebGlProgram, JsValue> {
    let vert = compile_shader(gl, vert_src, Gl::VERTEX_SHADER)?;
    let frag = compile_shader(gl, frag_src, Gl::FRAGMENT_SHADER)?;
    let program = gl
        .create_program()
        .ok_or_else(|| JsValue::from_str("unable to create program"))?;
    gl.attach_shader(&program, &vert);
    gl.attach_shader(&program, &frag);
    gl.link_program(&program);
    if gl
        .get_program_parameter(&program, Gl::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(JsValue::from_str(
            &gl.get_program_info_log(&program).unwrap_or_default(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Solid-color shaders (no texture sampling)
// ---------------------------------------------------------------------------

const SOLID_VERT_SHADER: &str = r#"#version 300 es
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec4 a_color;
layout(location = 3) in float a_flags;
uniform vec2 u_resolution;
out vec2 v_uv;
out vec4 v_color;
out float v_flags;
void main() {
  vec2 zeroToOne = a_pos / u_resolution;
  vec2 zeroToTwo = zeroToOne * 2.0;
  vec2 clipSpace = zeroToTwo - 1.0;
  gl_Position = vec4(clipSpace.x, -clipSpace.y, 0.0, 1.0);
  v_uv = a_uv;
  v_color = a_color;
  v_flags = a_flags;
}
"#;

const SOLID_FRAG_SHADER: &str = r#"#version 300 es
precision mediump float;
in vec4 v_color;
out vec4 fragColor;
void main() {
  fragColor = v_color;
}
"#;

// ---------------------------------------------------------------------------
// Textured (atlas) shaders — used for TextAtlas and IconAtlas materials
// ---------------------------------------------------------------------------

const TEXT_VERT_SHADER: &str = r#"#version 300 es
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec4 a_color;
layout(location = 3) in float a_flags;
uniform vec2 u_resolution;
out vec2 v_uv;
out vec4 v_color;
out float v_flags;
void main() {
  vec2 zeroToOne = a_pos / u_resolution;
  vec2 zeroToTwo = zeroToOne * 2.0;
  vec2 clipSpace = zeroToTwo - 1.0;
  gl_Position = vec4(clipSpace.x, -clipSpace.y, 0.0, 1.0);
  v_uv = a_uv;
  v_color = a_color;
  v_flags = a_flags;
}
"#;

const TEXT_FRAG_SHADER: &str = r#"#version 300 es
precision mediump float;
in vec2 v_uv;
in vec4 v_color;
uniform sampler2D u_atlas;
out vec4 fragColor;
void main() {
  float a = texture(u_atlas, v_uv).r;
  fragColor = vec4(v_color.rgb, v_color.a * a);
}
"#;
