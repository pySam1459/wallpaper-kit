// Diagnostic code-value sampling. Scene material conventions are separate.
Texture2D<float4> image : register(t0);
cbuffer Output : register(b0) {
    uint2 output_extent;
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
    uint width, height;
    image.GetDimensions(width, height);
    uint2 texel = ((uint2(position.xy) * 2 + 1) * uint2(width, height)) / (output_extent * 2);
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
