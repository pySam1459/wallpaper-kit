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
    uint layer_linear;
    float4 particle_color;
    float4 particle_uv_x;
    float4 particle_uv_y;
    float4 particle_next_uv_x;
    float2 particle_next_uv_y;
    float particle_frame_blend;
    uint particle_next_grayscale;
    uint2 particle_next_origin;
    uint2 particle_next_extent;
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

float4 cloud_sample(Texture2D<float4> source, float2 uv, uint2 origin, uint2 extent, bool repeat);

float4 layer_pixel(LayerVertex input) : SV_Target {
    uint2 texel = layer_source_origin + min(uint2(saturate(input.uv) * layer_source_extent), layer_source_extent - 1);
    float4 color = layer_linear != 0
        ? cloud_sample(image, input.uv, layer_source_origin, layer_source_extent, false)
        : image.Load(int3(texel,0));
    if (layer_grayscale != 0) { color = float4(color.rrr,1); }
    return float4(color.rgb * layer_brightness, color.a * layer_opacity);
}

Texture2D<float4> cloud_noise : register(t3);
Texture2D<float4> cloud_mask : register(t4);
cbuffer Cloud : register(b3) {
    float2 cloud_displacement;
    float2 cloud_noise_scale;
    float cloud_phase;
    uint cloud_has_mask;
    uint2 cloud_noise_origin;
    uint2 cloud_noise_extent;
    uint2 cloud_mask_origin;
    uint2 cloud_mask_extent;
    uint2 cloud_padding;
};

int2 cloud_texel(int2 coordinate, int2 extent, bool repeat) {
    return repeat ? ((coordinate % extent) + extent) % extent : clamp(coordinate, 0, extent - 1);
}

// Bilinear crop-local sampling. Repeat never reaches adjacent atlas pixels.
float4 cloud_sample(Texture2D<float4> source, float2 uv, uint2 origin, uint2 extent, bool repeat) {
    float2 position = uv * extent - 0.5;
    int2 base = int2(floor(position));
    float2 weight = frac(position);
    float4 a = source.Load(int3(int2(origin) + cloud_texel(base, int2(extent), repeat), 0));
    float4 b = source.Load(int3(int2(origin) + cloud_texel(base + int2(1,0), int2(extent), repeat), 0));
    float4 c = source.Load(int3(int2(origin) + cloud_texel(base + int2(0,1), int2(extent), repeat), 0));
    float4 d = source.Load(int3(int2(origin) + cloud_texel(base + int2(1,1), int2(extent), repeat), 0));
    return lerp(lerp(a,b,weight.x),lerp(c,d,weight.x),weight.y);
}

Texture2D<float4> particle_next : register(t7);

float4 particle_pixel(LayerVertex input) : SV_Target {
    float2 uv = particle_uv_x.xy + input.uv.x * particle_uv_x.zw + input.uv.y * particle_uv_y.xy;
    float4 color = cloud_sample(image, uv, layer_source_origin, layer_source_extent, false);
    if (layer_grayscale != 0) { color = float4(color.rrr,1); }
    if (particle_frame_blend > 0) {
        float2 next_uv = particle_next_uv_x.xy + input.uv.x * particle_next_uv_x.zw + input.uv.y * particle_next_uv_y;
        float4 next_color = cloud_sample(particle_next, next_uv, particle_next_origin, particle_next_extent, false);
        if (particle_next_grayscale != 0) { next_color = float4(next_color.rrr,1); }
        color = lerp(color, next_color, particle_frame_blend);
    }
    return float4(color.rgb * particle_color.rgb * layer_brightness, color.a * layer_opacity);
}

cbuffer Flow : register(b5) {
    float flow_strength;
    float flow_feather;
    float flow_time;
    float flow_phase_scale;
    uint2 flow_mask_extent;
    uint2 flow_phase_extent;
};

cbuffer Rays : register(b6) {
    float4 ray_h0;
    float4 ray_h1;
    float3 ray_start_color;
    float ray_exponent;
    float3 ray_end_color;
    float ray_intensity;
    float2 ray_feather;
    float ray_radius;
    float ray_smoothness;
    float2 ray_scale;
    float ray_noise_amount;
    float ray_noise_scale;
    float ray_time;
    uint ray_padding;
    uint2 ray_noise_extent;
    uint2 ray_gradient_extent;
    uint2 ray_padding2;
};

LayerVertex ray_vertex(uint id : SV_VertexID) {
    const float2 corners[6] = { float2(0,0),float2(1,0),float2(0,1),float2(0,1),float2(1,0),float2(1,1) };
    LayerVertex result;
    result.uv=corners[id];
    float w=1+ray_h1.z*result.uv.x+ray_h1.w*result.uv.y;
    float2 projected=float2(dot(ray_h0.xyz,float3(result.uv,1)),ray_h0.w*result.uv.x+ray_h1.x*result.uv.y+ray_h1.y)/w;
    float2 position=layer_center+layer_axis_x*(projected.x-0.5)+layer_axis_y*(projected.y-0.5);
    result.position=float4((position/canvas_extent*2-1)*w,0,w);
    return result;
}

float4 ray_pixel(LayerVertex input) : SV_Target {
    float2 from_corner=float2(1-input.uv.x,input.uv.y);
    float distance=length(from_corner);
    float angle=atan2(from_corner.y,max(from_corner.x,0.00001))*0.636619772;
    float distortion=cloud_sample(cloud_noise,input.uv*ray_noise_scale+ray_time,uint2(0,0),ray_noise_extent,true).r*2-1;
    float2 noise_uv=float2(angle/ray_scale.x+distortion*ray_noise_amount,distance/ray_scale.y+ray_time);
    float noise=0;
    for(int i=-1;i<=1;++i) { noise+=cloud_sample(cloud_noise,noise_uv+float2(i*ray_radius*ray_smoothness,0),uint2(0,0),ray_noise_extent,true).r/3; }
    float signal=pow(saturate(noise),max(ray_exponent,0.01)*6)*ray_intensity;
    float2 feather=saturate((1-input.uv)/max(ray_feather,0.00001));
    signal*=feather.x*feather.y;
    float3 gradient=cloud_sample(cloud_mask,float2(angle,0.5),uint2(0,0),ray_gradient_extent,false).rgb;
    float3 color=lerp(ray_start_color,ray_end_color,saturate(distance))*gradient;
    return float4(color,saturate(signal));
}

float4 flow_pixel(LayerVertex input) : SV_Target {
    float2 field = (cloud_sample(cloud_noise, input.uv, uint2(0,0), flow_mask_extent, false).rg * 255 - 127) / 128;
    // Two advection phases conceal each other's reset. The painted neutral
    // value 127 produces exactly zero displacement, preserving unpainted art.
    float phase_noise = cloud_sample(cloud_mask, input.uv * layer_source_extent * flow_phase_scale, uint2(0,0), flow_phase_extent, true).r;
    float phase = frac(flow_time + phase_noise);
    float second = frac(phase + 0.5);
    float2 displacement = field * flow_strength * 0.05;
    float4 a = cloud_sample(image, input.uv - displacement * (phase - 0.5), layer_source_origin, layer_source_extent, false);
    float4 b = cloud_sample(image, input.uv - displacement * (second - 0.5), layer_source_origin, layer_source_extent, false);
    float weight = abs(phase * 2 - 1);
    weight = lerp(weight, smoothstep(0, 1, weight), flow_feather);
    float4 color = lerp(a, b, weight);
    return float4(color.rgb * layer_brightness, color.a * layer_opacity);
}

Texture2D<float4> scene_image : register(t5);
Texture2D<float4> scene_aux : register(t6);
cbuffer Scene : register(b4) {
    uint2 scene_extent;
    uint2 scene_source_extent;
    float refract_amount;
    float bloom_threshold;
    float bloom_feather;
    float bloom_scatter;
    float bloom_strength;
    uint bloom_first;
    uint2 scene_padding;
};

float4 refract_pixel(LayerVertex input) : SV_Target {
    float4 normal = cloud_sample(scene_aux, input.uv, uint2(0,0), scene_source_extent, false);
    float2 offset = normal.xy * 2 - 1;
    float2 axis_x = normalize(layer_axis_x);
    float2 axis_y = normalize(layer_axis_y);
    offset = float2(axis_x.x * offset.x + axis_y.x * offset.y,
                  -axis_x.y * offset.x - axis_y.y * offset.y);
    float4 albedo = cloud_sample(image, input.uv, layer_source_origin, layer_source_extent, false);
    float2 uv = input.position.xy / scene_extent + offset * refract_amount;
    float4 color = cloud_sample(scene_image, uv, uint2(0,0), scene_extent, false);
    return float4(color.rgb, albedo.a * layer_opacity);
}

float3 bloom_prefilter(float3 color) {
    float brightness = max(color.r, max(color.g, color.b));
    float knee = bloom_threshold * bloom_feather;
    float soft = clamp(brightness - bloom_threshold + knee, 0, 2 * knee);
    soft = soft * soft / max(4 * knee, 0.00001);
    float contribution = max(soft, brightness - bloom_threshold) / max(brightness, 0.00001);
    return color * max(contribution, 0);
}

float4 bloom_down(Vertex input) : SV_Target {
    float2 uv = input.position.xy / scene_extent;
    float2 texel = 1.0 / scene_source_extent;
    float3 color = 0;
    for (int y = -1; y <= 1; ++y) {
        for (int x = -1; x <= 1; ++x) {
            float weight = (x == 0 ? 2 : 1) * (y == 0 ? 2 : 1) / 16.0;
            float3 sample_color = cloud_sample(scene_image, uv + float2(x,y) * texel, uint2(0,0), scene_source_extent, false).rgb;
            color += (bloom_first != 0 ? bloom_prefilter(sample_color) : sample_color) * weight;
        }
    }
    return float4(color,1);
}

float4 bloom_up(Vertex input) : SV_Target {
    float2 uv = input.position.xy / scene_extent;
    float2 texel = 1.0 / scene_source_extent;
    float3 color = 0;
    for (int y = -1; y <= 1; ++y) {
        for (int x = -1; x <= 1; ++x) {
            float weight = (x == 0 ? 2 : 1) * (y == 0 ? 2 : 1) / 16.0;
            color += cloud_sample(scene_image, uv + float2(x,y) * texel, uint2(0,0), scene_source_extent, false).rgb * weight;
        }
    }
    return float4(color * bloom_scatter,1);
}

float4 resolve_pixel(Vertex input) : SV_Target {
    float3 color = scene_image.Load(int3(uint2(input.position.xy),0)).rgb;
    if (bloom_first != 0) {
        uint width, height;
        scene_aux.GetDimensions(width,height);
        color += cloud_sample(scene_aux, input.position.xy / scene_extent, uint2(0,0), uint2(width,height), false).rgb * bloom_strength;
    }
    return float4(color,1);
}

// Independently derived from generated reference gradients and timed probes.
float4 cloud_pixel(LayerVertex input) : SV_Target {
    float2 noise_uv = input.uv * cloud_noise_scale + float2(cloud_phase,0);
    float noise = cloud_sample(cloud_noise, noise_uv, cloud_noise_origin, cloud_noise_extent, true).r * 2 - 1;
    float mask = cloud_has_mask != 0 ? cloud_sample(cloud_mask, input.uv, cloud_mask_origin, cloud_mask_extent, false).r : 1;
    float2 displaced = input.uv + cloud_displacement * noise * mask * mask;
    float4 color = cloud_sample(image, displaced, layer_source_origin, layer_source_extent, false);
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
