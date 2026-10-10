// Diagnostic code-value sampling. Scene material conventions are separate.
Texture2D<float4> image : register(t0);
cbuffer Output : register(b0) {
    uint2 output_extent;
    uint2 source_origin;
    uint2 source_extent;
    uint2 padding;
};

struct Vertex {
    float4 position : SV_Position;
};

Vertex vertex_main(uint id : SV_VertexID) {
    Vertex result;
    float2 uv = float2((id << 1) & 2, id & 2);
    result.position = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
    return result;
}

float4 sample_page(float4 position) {
    uint2 texel = source_origin + ((uint2(position.xy) * 2 + 1) * source_extent) / (output_extent * 2);
    return image.Load(int3(texel, 0));
}

float4 rgba_main(Vertex input) : SV_Target {
    float4 color = sample_page(input.position);
    return float4(color.rgb * color.a + float3(16, 24, 32) / 255.0 * (1 - color.a), 1);
}

float4 r8_main(Vertex input) : SV_Target {
    float value = sample_page(input.position).r;
    return float4(value, value, value, 1);
}

cbuffer Layer : register(b1) {
    float2 canvas_extent;
    float2 layer_center;
    float2 layer_axis_x;
    float2 layer_axis_y;
    uint2 layer_source_origin;
    uint2 layer_source_extent;
    float layer_opacity;
    float layer_brightness;
    uint layer_grayscale;
    uint layer_padding;
};

struct LayerVertex {
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

LayerVertex layer_vertex(uint id : SV_VertexID) {
    const float2 corners[6] = { float2(0,0),float2(1,0),float2(0,1),float2(0,1),float2(1,0),float2(1,1) };
    LayerVertex result;
    result.uv = corners[id];
    float2 position = layer_center + layer_axis_x * (result.uv.x - 0.5) + layer_axis_y * (result.uv.y - 0.5);
    result.position = float4(position / canvas_extent * 2 - 1,0,1);
    return result;
}

float4 layer_pixel(LayerVertex input) : SV_Target {
    uint2 texel = layer_source_origin + min(uint2(saturate(input.uv) * layer_source_extent), layer_source_extent - 1);
    float4 color = image.Load(int3(texel,0));
    if (layer_grayscale != 0) { color = float4(color.rrr,1); }
    return float4(color.rgb * layer_brightness, color.a * layer_opacity);
}

Texture2D<float4> horizontal_image : register(t1);
Texture2D<float4> blur_mask : register(t2);
cbuffer Blur : register(b2) {
    float2 blur_sigma;
    uint blur_has_mask;
    uint blur_profile; // 0: premultiplied sigma; 1: straight-RGBA precise scale
    uint2 blur_mask_origin;
    uint2 blur_mask_extent;
};

// Independent normalized Gaussian filtering in premultiplied code values.
// Each axis uses ceil(3*sigma) taps on either side, at most 24.
float4 original_premultiplied(int2 texel) {
    texel = clamp(texel, int2(0,0), int2(layer_source_extent) - 1);
    float4 color = image.Load(int3(int2(layer_source_origin) + texel,0));
    if (layer_grayscale != 0) { color = float4(color.rrr,1); }
    return blur_profile == 0 ? float4(color.rgb * color.a,color.a) : color;
}

// Crop-clamped bilinear sampling expressed with loads, including BC3 crops.
float4 precise_source(float2 position) {
    int2 base = int2(floor(position));
    float2 fraction = frac(position);
    return lerp(lerp(original_premultiplied(base),original_premultiplied(base + int2(1,0)),fraction.x),
                lerp(original_premultiplied(base + int2(0,1)),original_premultiplied(base + int2(1,1)),fraction.x),fraction.y);
}

float4 precise_scratch(float2 position) {
    int2 base = int2(floor(position));
    float2 fraction = frac(position);
    int2 maximum = int2(layer_source_extent) - 1;
    return lerp(lerp(horizontal_image.Load(int3(clamp(base,0,maximum),0)),
                     horizontal_image.Load(int3(clamp(base + int2(1,0),0,maximum),0)),fraction.x),
                lerp(horizontal_image.Load(int3(clamp(base + int2(0,1),0,maximum),0)),
                     horizontal_image.Load(int3(clamp(base + int2(1,1),0,maximum),0)),fraction.x),fraction.y);
}

// Paired 13-texel kernel adapted from MIT glsl-fast-gaussian-blur/13.glsl.
// Copyright (c) 2015 Jam3. See THIRD-PARTY-NOTICES.md for license and source.
float4 precise_filter(float2 position, bool horizontal) {
    const float offsets[3] = { 1.411764705882353,3.2941176470588234,5.176470588235294 };
    const float weights[3] = { 0.2969069646728344,0.09447039785044732,0.010381362401148057 };
    float2 direction = horizontal ? float2(blur_sigma.x,0) : float2(0,blur_sigma.y);
    float4 color = (horizontal ? precise_source(position) : precise_scratch(position)) * 0.1964825501511404;
    for (uint i = 0; i < 3; ++i) {
        float2 offset = direction * offsets[i];
        color += (horizontal ? precise_source(position + offset) : precise_scratch(position + offset)) * weights[i];
        color += (horizontal ? precise_source(position - offset) : precise_scratch(position - offset)) * weights[i];
    }
    return color;
}

float4 blur_horizontal(Vertex input) : SV_Target {
    int2 texel = int2(input.position.xy);
    if (blur_profile != 0) { return round(saturate(precise_filter(float2(texel),true)) * 255) / 255; }
    if (blur_sigma.x == 0) { return original_premultiplied(texel); }
    int radius = int(ceil(3 * blur_sigma.x));
    float4 color = 0;
    float total = 0;
    for (int dx = -radius; dx <= radius; ++dx) {
        float distance = float(dx) / blur_sigma.x;
        float weight = exp(-0.5 * distance * distance);
        color += original_premultiplied(texel + int2(dx,0)) * weight;
        total += weight;
    }
    return color / total;
}

float4 blur_vertical(LayerVertex input) : SV_Target {
    uint2 texel = min(uint2(saturate(input.uv) * layer_source_extent),layer_source_extent - 1);
    if (blur_profile != 0) {
        float4 precise = precise_filter(float2(texel),false);
        return float4(precise.rgb * layer_brightness,precise.a * layer_opacity);
    }
    float4 color = horizontal_image.Load(int3(texel,0));
    if (blur_sigma.y != 0) {
        int radius = int(ceil(3 * blur_sigma.y));
        color = 0;
        float total = 0;
        for (int dy = -radius; dy <= radius; ++dy) {
            int2 sample_texel = int2(texel.x,clamp(int(texel.y) + dy,0,int(layer_source_extent.y) - 1));
            float distance = float(dy) / blur_sigma.y;
            float weight = exp(-0.5 * distance * distance);
            color += horizontal_image.Load(int3(sample_texel,0)) * weight;
            total += weight;
        }
        color /= total;
    }
    if (blur_has_mask != 0) {
        uint2 mask_texel = blur_mask_origin + min(uint2(saturate(input.uv) * blur_mask_extent),blur_mask_extent - 1);
        color = lerp(original_premultiplied(int2(texel)),color,saturate(blur_mask.Load(int3(mask_texel,0)).r));
    }
    float3 straight_rgb = color.a > 0 ? color.rgb / color.a : 0;
    return float4(straight_rgb * layer_brightness,color.a * layer_opacity);
}
