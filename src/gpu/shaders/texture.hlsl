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
