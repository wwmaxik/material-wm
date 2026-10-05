use smithay::backend::renderer::gles::{
    element::PixelShaderElement, GlesError, GlesPixelProgram, GlesRenderer, Uniform, UniformName,
    UniformType,
};
use smithay::backend::renderer::element::Kind;
use smithay::utils::{Logical, Rectangle};

pub const PILL_FRAG_SRC: &str = r#"
precision mediump float;
varying vec2 v_coords;
uniform vec2 size;
uniform float alpha;
uniform float radius;
uniform vec4 color;

float rounded_box(vec2 p, vec2 b, float r) {
    vec2 d = abs(p) - b + vec2(r);
    return min(max(d.x, d.y), 0.0) + length(max(d, 0.0)) - r;
}

void main() {
    vec2 half_size = size * 0.5;
    vec2 p = (v_coords * size) - half_size;
    float d = rounded_box(p, half_size, radius);
    float a = 1.0 - smoothstep(-0.8, 0.8, d);
    gl_FragColor = color * (a * alpha);
}
"#;

pub const BORDER_FRAG_SRC: &str = r#"
precision mediump float;
varying vec2 v_coords;
uniform vec2 size;
uniform float alpha;
uniform float radius;
uniform float border_width;
uniform vec4 border_color;
uniform vec4 bg_color;

float rounded_box(vec2 p, vec2 b, float r) {
    vec2 d = abs(p) - b + vec2(r);
    return min(max(d.x, d.y), 0.0) + length(max(d, 0.0)) - r;
}

void main() {
    vec2 half_size = size * 0.5;
    vec2 p = (v_coords * size) - half_size;
    float outer_d = rounded_box(p, half_size, radius);
    float inner_d = rounded_box(p, half_size - vec2(border_width), max(0.0, radius - border_width));
    float outer_mask = 1.0 - smoothstep(-0.8, 0.8, outer_d);
    float inner_mask = 1.0 - smoothstep(-0.8, 0.8, inner_d);
    float border_mask = outer_mask - inner_mask;
    vec4 result = mix(bg_color * inner_mask, border_color, border_mask);
    gl_FragColor = result * alpha;
}
"#;

pub const SHADOW_FRAG_SRC: &str = r#"
precision mediump float;
varying vec2 v_coords;
uniform vec2 size;
uniform float alpha;
uniform float radius;
uniform float blur;
uniform vec4 shadow_color;

float rounded_box(vec2 p, vec2 b, float r) {
    vec2 d = abs(p) - b + vec2(r);
    return min(max(d.x, d.y), 0.0) + length(max(d, 0.0)) - r;
}

void main() {
    vec2 half_size = (size - vec2(blur * 2.0)) * 0.5;
    vec2 p = (v_coords * size) - (size * 0.5);
    float d = rounded_box(p, half_size, radius);
    float shadow = 1.0 - smoothstep(-blur * 0.5, blur * 1.5, d);
    gl_FragColor = shadow_color * (shadow * alpha);
}
"#;

#[derive(Clone)]
pub struct MaterialShaderPipeline {
    pub pill_shader: GlesPixelProgram,
    pub border_shader: GlesPixelProgram,
    pub shadow_shader: GlesPixelProgram,
}

impl MaterialShaderPipeline {
    pub fn new(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        let pill_uniforms = [
            UniformName {
                name: "radius".into(),
                type_: UniformType::_1f,
            },
            UniformName {
                name: "color".into(),
                type_: UniformType::_4f,
            },
        ];
        let pill_shader = renderer.compile_custom_pixel_shader(PILL_FRAG_SRC, &pill_uniforms)?;

        let border_uniforms = [
            UniformName {
                name: "radius".into(),
                type_: UniformType::_1f,
            },
            UniformName {
                name: "border_width".into(),
                type_: UniformType::_1f,
            },
            UniformName {
                name: "border_color".into(),
                type_: UniformType::_4f,
            },
            UniformName {
                name: "bg_color".into(),
                type_: UniformType::_4f,
            },
        ];
        let border_shader = renderer.compile_custom_pixel_shader(BORDER_FRAG_SRC, &border_uniforms)?;

        let shadow_uniforms = [
            UniformName {
                name: "radius".into(),
                type_: UniformType::_1f,
            },
            UniformName {
                name: "blur".into(),
                type_: UniformType::_1f,
            },
            UniformName {
                name: "shadow_color".into(),
                type_: UniformType::_4f,
            },
        ];
        let shadow_shader = renderer.compile_custom_pixel_shader(SHADOW_FRAG_SRC, &shadow_uniforms)?;

        Ok(Self {
            pill_shader,
            border_shader,
            shadow_shader,
        })
    }

    /// Construct a filled rounded rectangle / pill element
    pub fn create_pill_element(
        &self,
        rect: Rectangle<i32, Logical>,
        radius: f32,
        color: [f32; 4],
        alpha: f32,
    ) -> PixelShaderElement {
        let uniforms = vec![
            Uniform::new("radius", radius),
            Uniform::new("color", color),
        ];

        PixelShaderElement::new(
            self.pill_shader.clone(),
            rect,
            None,
            alpha,
            uniforms,
            Kind::Unspecified,
        )
    }

    /// Construct a bordered rounded rectangle element for window boundaries
    pub fn create_border_element(
        &self,
        rect: Rectangle<i32, Logical>,
        radius: f32,
        border_width: f32,
        border_color: [f32; 4],
        bg_color: [f32; 4],
        alpha: f32,
    ) -> PixelShaderElement {
        let uniforms = vec![
            Uniform::new("radius", radius),
            Uniform::new("border_width", border_width),
            Uniform::new("border_color", border_color),
            Uniform::new("bg_color", bg_color),
        ];

        PixelShaderElement::new(
            self.border_shader.clone(),
            rect,
            None,
            alpha,
            uniforms,
            Kind::Unspecified,
        )
    }

    /// Construct a 9-slice / SDF elevation drop shadow element
    pub fn create_shadow_element(
        &self,
        rect: Rectangle<i32, Logical>,
        radius: f32,
        blur: f32,
        shadow_color: [f32; 4],
        alpha: f32,
    ) -> PixelShaderElement {
        // Expand rect by blur padding on all 4 sides
        let padding = blur as i32;
        let shadow_rect = Rectangle::new(
            (rect.loc.x - padding, rect.loc.y - padding).into(),
            (rect.size.w + padding * 2, rect.size.h + padding * 2).into(),
        );

        let uniforms = vec![
            Uniform::new("radius", radius),
            Uniform::new("blur", blur),
            Uniform::new("shadow_color", shadow_color),
        ];

        PixelShaderElement::new(
            self.shadow_shader.clone(),
            shadow_rect,
            None,
            alpha,
            uniforms,
            Kind::Unspecified,
        )
    }
}
