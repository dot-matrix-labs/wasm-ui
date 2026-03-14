use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram, WebGlShader, WebGlTexture, WebGlUniformLocation};

use ui_core::batch::{Batch, Material, Quad, TextRun};
use ui_core::types::Rect;

use crate::atlas::TextAtlas;

pub struct Renderer {
    gl: Gl,
    program: WebGlProgram,
    /// Double-buffered VBOs to avoid GPU pipeline stalls (Task 4.4).
    vbos: [WebGlBuffer; 2],
    vbo_index: usize,
    ibo: WebGlBuffer,
    atlas: TextAtlas,
    atlas_texture: WebGlTexture,
    width: f32,
    height: f32,
    // Cached uniform locations (Task 4.2).
    u_resolution: Option<WebGlUniformLocation>,
    u_use_texture: Option<WebGlUniformLocation>,
    u_atlas: Option<WebGlUniformLocation>,
    /// Task 4.1: tracks whether the VBOs have valid data from the previous frame.
    vbo_populated: bool,
    // Task 4.3: Instanced rendering for solid-color quads.
    /// Whether instanced rendering is available (always true in WebGL2, but we
    /// still gate on a successful program compile so failures degrade gracefully).
    instanced_available: bool,
    /// Shader program used for instanced solid quads.
    inst_program: Option<WebGlProgram>,
    /// Unit quad template VBO (two triangles, positions 0..1).
    inst_quad_vbo: Option<WebGlBuffer>,
    /// Per-instance data VBO (x, y, w, h, r, g, b, a — 8 floats per quad).
    inst_data_vbo: Option<WebGlBuffer>,
    /// Cached uniform locations for the instanced program.
    inst_u_resolution: Option<WebGlUniformLocation>,
}

impl Renderer {
    pub fn new(canvas: &HtmlCanvasElement, width: f32, height: f32) -> Result<Self, JsValue> {
        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("WebGL2 not supported"))?
            .dyn_into()?;
        let program = link_program(&gl, VERT_SHADER, FRAG_SHADER)?;
        let vbo0 = gl.create_buffer().ok_or_else(|| JsValue::from_str("no vbo0"))?;
        let vbo1 = gl.create_buffer().ok_or_else(|| JsValue::from_str("no vbo1"))?;
        let ibo = gl.create_buffer().ok_or_else(|| JsValue::from_str("no ibo"))?;
        let atlas_texture = gl.create_texture().ok_or_else(|| JsValue::from_str("no texture"))?;

        gl.use_program(Some(&program));
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        // Cache uniform locations at init time (Task 4.2).
        let u_resolution = gl.get_uniform_location(&program, "u_resolution");
        let u_use_texture = gl.get_uniform_location(&program, "u_use_texture");
        let u_atlas = gl.get_uniform_location(&program, "u_atlas");

        // Task 4.3: Set up instanced rendering for solid quads.
        // In WebGL2 instancing is always available; fall back gracefully if
        // the program fails to compile.
        let (instanced_available, inst_program, inst_quad_vbo, inst_data_vbo, inst_u_resolution) =
            Self::init_instanced(&gl);

        let mut renderer = Self {
            gl,
            program,
            vbos: [vbo0, vbo1],
            vbo_index: 0,
            ibo,
            atlas: TextAtlas::new(1024, 1024),
            atlas_texture,
            width,
            height,
            u_resolution,
            u_use_texture,
            u_atlas,
            vbo_populated: false,
            instanced_available,
            inst_program,
            inst_quad_vbo,
            inst_data_vbo,
            inst_u_resolution,
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

    /// Add a fallback font that is tried when the primary font is missing a
    /// glyph.  Multiple fallbacks can be added and are tried in insertion order.
    /// Task 2.6: Font Fallback Chain.
    pub fn add_fallback_font_bytes(&mut self, bytes: Vec<u8>) {
        self.atlas.add_fallback_font_bytes(bytes);
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
        // Task 4.5: skip upload entirely if nothing changed.
        if !self.atlas.is_dirty() {
            return;
        }
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas_texture));

        if let Some((rx, ry, rw, rh)) = self.atlas.dirty_rect() {
            // Partial upload: only the dirtied sub-region.
            let atlas_width = 1024u32;
            let all_pixels = self.atlas.pixels();
            // Extract the dirty rows into a contiguous sub-buffer.
            let mut sub: Vec<u8> = Vec::with_capacity((rw * rh) as usize);
            for row in 0..rh {
                let src_start = ((ry + row) * atlas_width + rx) as usize;
                sub.extend_from_slice(&all_pixels[src_start..src_start + rw as usize]);
            }
            gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                rx as i32,
                ry as i32,
                rw as i32,
                rh as i32,
                Gl::RED,
                Gl::UNSIGNED_BYTE,
                Some(&sub),
            )
            .ok();
        } else {
            // No dirty rect recorded — fall back to full upload.
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
        }
        self.atlas.mark_clean();
    }

    fn draw_batch(&mut self, batch: &Batch) -> Result<(), JsValue> {
        let gl = &self.gl;
        gl.use_program(Some(&self.program));

        // Use cached uniform location (Task 4.2).
        if let Some(loc) = &self.u_resolution {
            gl.uniform2f(Some(loc), self.width, self.height);
        }

        // Task 4.1: Skip VBO upload if nothing in the batch changed.
        // We still call draw below so the cached GPU data is rendered.
        if batch.dirty || !self.vbo_populated {
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
            let index_data = &batch.indices;

            // Task 4.4: Alternate VBOs (double-buffering) to avoid GPU pipeline stalls.
            self.vbo_index = 1 - self.vbo_index;
            let current_vbo = &self.vbos[self.vbo_index];

            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(current_vbo));
            // Orphan the buffer before uploading new data.
            gl.buffer_data_with_i32(Gl::ARRAY_BUFFER, 0, Gl::DYNAMIC_DRAW);
            unsafe {
                let vert_array = js_sys::Float32Array::view(&vertex_data);
                gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &vert_array, Gl::DYNAMIC_DRAW);
            }
            gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.ibo));
            unsafe {
                let idx_array = js_sys::Uint32Array::view(index_data);
                gl.buffer_data_with_array_buffer_view(Gl::ELEMENT_ARRAY_BUFFER, &idx_array, Gl::DYNAMIC_DRAW);
            }
            self.vbo_populated = true;
        } else {
            // Re-bind the current VBO so attribute pointers are set correctly below.
            let current_vbo = &self.vbos[self.vbo_index];
            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(current_vbo));
            gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.ibo));
        }

        let stride = 9 * 4;
        let a_pos = gl.get_attrib_location(&self.program, "a_pos") as u32;
        let a_uv = gl.get_attrib_location(&self.program, "a_uv") as u32;
        let a_color = gl.get_attrib_location(&self.program, "a_color") as u32;
        let a_flags = gl.get_attrib_location(&self.program, "a_flags") as u32;

        gl.enable_vertex_attrib_array(a_pos);
        gl.vertex_attrib_pointer_with_i32(a_pos, 2, Gl::FLOAT, false, stride, 0);

        gl.enable_vertex_attrib_array(a_uv);
        gl.vertex_attrib_pointer_with_i32(a_uv, 2, Gl::FLOAT, false, stride, 2 * 4);

        gl.enable_vertex_attrib_array(a_color);
        gl.vertex_attrib_pointer_with_i32(a_color, 4, Gl::FLOAT, false, stride, 4 * 4);

        gl.enable_vertex_attrib_array(a_flags);
        gl.vertex_attrib_pointer_with_i32(a_flags, 1, Gl::FLOAT, false, stride, 8 * 4);

        gl.clear_color(0.97, 0.97, 0.96, 1.0);
        gl.clear(Gl::COLOR_BUFFER_BIT);

        for cmd in &batch.commands {
            // Task 4.3: solid quads are handled by the instanced path when available.
            if self.instanced_available && cmd.material == Material::Solid {
                continue;
            }

            match cmd.material {
                Material::TextAtlas => self.bind_text_texture(),
                Material::Solid => self.unbind_text_texture(),
                Material::IconAtlas => self.bind_text_texture(),
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

        // Task 4.3: Draw solid quads via instanced rendering.
        if self.instanced_available {
            self.draw_solid_instanced(batch);
        }

        Ok(())
    }

    fn bind_text_texture(&self) {
        let gl = &self.gl;
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas_texture));
        // Use cached uniform locations (Task 4.2).
        if let Some(loc) = &self.u_use_texture {
            gl.uniform1i(Some(loc), 1);
        }
        if let Some(loc) = &self.u_atlas {
            gl.uniform1i(Some(loc), 0);
        }
    }

    fn unbind_text_texture(&self) {
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, None);
        // Use cached uniform location (Task 4.2).
        if let Some(loc) = &self.u_use_texture {
            gl.uniform1i(Some(loc), 0);
        }
    }

    // Task 4.3: Instanced rendering helpers.

    /// Initialize the instanced solid-quad rendering resources.
    /// Returns `(available, program, quad_vbo, data_vbo, u_resolution)`.
    fn init_instanced(
        gl: &Gl,
    ) -> (bool, Option<WebGlProgram>, Option<WebGlBuffer>, Option<WebGlBuffer>, Option<WebGlUniformLocation>) {
        // In WebGL2 instancing is built-in (no extension needed), but we check
        // ANGLE_instanced_arrays for completeness and degrade gracefully.
        // WebGL2 always supports instancing so the extension check is advisory.
        let _ext = gl.get_extension("ANGLE_instanced_arrays"); // returns Result<Option<Object>>

        let prog = match link_program(gl, INST_VERT_SHADER, INST_FRAG_SHADER) {
            Ok(p) => p,
            Err(_) => return (false, None, None, None, None),
        };

        let quad_vbo = match gl.create_buffer() {
            Some(b) => b,
            None => return (false, None, None, None, None),
        };
        let data_vbo = match gl.create_buffer() {
            Some(b) => b,
            None => return (false, None, None, None, None),
        };

        // Unit quad: two triangles covering (0,0)..(1,1) in local space.
        let quad_verts: [f32; 12] = [
            0.0, 0.0,
            1.0, 0.0,
            1.0, 1.0,
            0.0, 0.0,
            1.0, 1.0,
            0.0, 1.0,
        ];
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&quad_vbo));
        unsafe {
            let arr = js_sys::Float32Array::view(&quad_verts);
            gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &arr, Gl::STATIC_DRAW);
        }

        let u_res = gl.get_uniform_location(&prog, "u_resolution");

        (true, Some(prog), Some(quad_vbo), Some(data_vbo), u_res)
    }

    /// Draw solid-color quads from the batch using instanced rendering.
    fn draw_solid_instanced(&self, batch: &Batch) {
        let (prog, quad_vbo, data_vbo) = match (&self.inst_program, &self.inst_quad_vbo, &self.inst_data_vbo) {
            (Some(p), Some(qv), Some(dv)) => (p, qv, dv),
            _ => return,
        };
        let gl = &self.gl;

        // Collect per-instance data for solid quads: x,y,w,h,r,g,b,a.
        let mut instances: Vec<f32> = Vec::new();
        for cmd in &batch.commands {
            if cmd.material != Material::Solid {
                continue;
            }
            // Each solid draw command covers `cmd.count/6` quads.
            let quad_count = cmd.count / 6;
            let base_vertex = (cmd.start / 6) * 4; // 4 verts per quad
            for q in 0..quad_count {
                let vi = (base_vertex + q * 4) as usize;
                if vi + 3 >= batch.vertices.len() {
                    break;
                }
                let tl = &batch.vertices[vi];
                let br = &batch.vertices[vi + 2];
                instances.push(tl.pos.x);
                instances.push(tl.pos.y);
                instances.push(br.pos.x - tl.pos.x);
                instances.push(br.pos.y - tl.pos.y);
                instances.push(tl.color.r);
                instances.push(tl.color.g);
                instances.push(tl.color.b);
                instances.push(tl.color.a);
            }
        }
        if instances.is_empty() {
            return;
        }
        let instance_count = (instances.len() / 8) as i32;

        gl.use_program(Some(prog));
        if let Some(loc) = &self.inst_u_resolution {
            gl.uniform1f(Some(loc), self.width); // packed: x=width, y=height below
        }
        // Re-use u_resolution as vec2.
        if let Some(loc) = &self.inst_u_resolution {
            gl.uniform2f(Some(loc), self.width, self.height);
        }

        // Upload instance data.
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(data_vbo));
        unsafe {
            let arr = js_sys::Float32Array::view(&instances);
            gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &arr, Gl::DYNAMIC_DRAW);
        }

        let a_corner = gl.get_attrib_location(prog, "a_corner") as u32;
        let a_rect   = gl.get_attrib_location(prog, "a_rect")   as u32;
        let a_color  = gl.get_attrib_location(prog, "a_color")  as u32;

        // Bind quad template VBO for corner attribute (attrib divisor 0).
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(quad_vbo));
        gl.enable_vertex_attrib_array(a_corner);
        gl.vertex_attrib_pointer_with_i32(a_corner, 2, Gl::FLOAT, false, 0, 0);
        gl.vertex_attrib_divisor(a_corner, 0);

        // Bind instance VBO for per-instance rect and color (attrib divisor 1).
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(data_vbo));
        let stride = 8 * 4; // 8 floats * 4 bytes
        gl.enable_vertex_attrib_array(a_rect);
        gl.vertex_attrib_pointer_with_i32(a_rect, 4, Gl::FLOAT, false, stride, 0);
        gl.vertex_attrib_divisor(a_rect, 1);

        gl.enable_vertex_attrib_array(a_color);
        gl.vertex_attrib_pointer_with_i32(a_color, 4, Gl::FLOAT, false, stride, 4 * 4);
        gl.vertex_attrib_divisor(a_color, 1);

        gl.disable(Gl::SCISSOR_TEST);
        gl.draw_arrays_instanced(Gl::TRIANGLES, 0, 6, instance_count);

        // Reset divisors to avoid polluting later draw calls.
        gl.vertex_attrib_divisor(a_rect, 0);
        gl.vertex_attrib_divisor(a_color, 0);
        gl.vertex_attrib_divisor(a_corner, 0);

        // Restore the regular program.
        gl.use_program(Some(&self.program));
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

const VERT_SHADER: &str = r#"
attribute vec2 a_pos;
attribute vec2 a_uv;
attribute vec4 a_color;
attribute float a_flags;
uniform vec2 u_resolution;
varying vec2 v_uv;
varying vec4 v_color;
varying float v_flags;
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

const FRAG_SHADER: &str = r#"
precision mediump float;
varying vec2 v_uv;
varying vec4 v_color;
varying float v_flags;
uniform sampler2D u_atlas;
uniform int u_use_texture;
void main() {
  if (u_use_texture == 1) {
    float dist = texture2D(u_atlas, v_uv).r;
    // Task 2.1: SDF-style smooth alpha for crisp text at any scale.
    // fontdue produces coverage bitmaps (0..1); treat values >= 0.5 as "inside"
    // using a smoothstep edge so we get anti-aliasing without a true SDF pass.
    // When a real SDF atlas is used the 0.5 threshold and width will stay valid.
    float edge_width = fwidth(dist) * 0.7;
    float a = smoothstep(0.5 - edge_width, 0.5 + edge_width, dist);
    gl_FragColor = vec4(v_color.rgb, v_color.a * a);
  } else {
    gl_FragColor = v_color;
  }
}
"#;

// Task 4.3: Instanced rendering shaders for solid-color quads.
// a_corner: unit quad corner (0..1 in x and y), divisor 0.
// a_rect:   per-instance (x, y, w, h) in screen pixels, divisor 1.
// a_color:  per-instance (r, g, b, a), divisor 1.
const INST_VERT_SHADER: &str = r#"
attribute vec2 a_corner;
attribute vec4 a_rect;
attribute vec4 a_color;
uniform vec2 u_resolution;
varying vec4 v_color;
void main() {
  vec2 pos = a_rect.xy + a_corner * a_rect.zw;
  vec2 clip = (pos / u_resolution) * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_color = a_color;
}
"#;

const INST_FRAG_SHADER: &str = r#"
precision mediump float;
varying vec4 v_color;
void main() {
  gl_FragColor = v_color;
}
"#;
