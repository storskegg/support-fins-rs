/// Metal source, compiled at run time (`MTLDevice.makeLibrary(source:)`) so the package builds the same under
/// `swift build`, `swift test` and Xcode.
enum Shaders {
    static let source = """
    #include <metal_stdlib>
    using namespace metal;

    struct Uniforms {
        float4x4 viewProj;
        float4x4 model;
        float4 eye;
        // x: hovered face (global; 0xFFFFFFFF = none), y: vertex stride in floats, z: highlight small overhangs,
        // w: this part's first global face
        uint4 params;
    };

    // Face flag bits, as `FaceFlags` in MeshSource.swift.
    constant uchar FLAG_OVER = 1;
    constant uchar FLAG_KEPT = 2;
    constant uchar FLAG_BED = 4;

    constant float3 PLAIN = float3(0.725, 0.761, 0.816);
    constant float3 OVERHANG = float3(1.0, 0.353, 0.302);
    constant float3 SMALL = float3(1.0, 0.706, 0.329);
    constant float3 BED = float3(0.247, 0.498, 0.816);
    constant float3 HOVER = float3(0.302, 0.639, 1.0);
    constant float3 BACK = float3(0.09, 0.10, 0.13);

    struct MeshOut {
        float4 position [[position]];
        float3 normal;
        float3 color [[flat]];
    };

    vertex MeshOut mesh_vertex(uint vid [[vertex_id]],
                               const device float* verts [[buffer(0)]],
                               const device uchar* flags [[buffer(1)]],
                               constant Uniforms& u [[buffer(2)]]) {
        uint o = vid * u.params.y;
        float3 p = float3(verts[o], verts[o + 1], verts[o + 2]);
        float3 n = float3(verts[o + 3], verts[o + 4], verts[o + 5]);
        uint face = vid / 3;
        uchar f = flags[face];
        // web/ui/part.js paintOverhangs: kept = red, else over (a sliver) = amber, else bed contact = blue
        float3 c = PLAIN;
        if (f & FLAG_KEPT) c = OVERHANG;
        else if ((f & FLAG_OVER) && u.params.z != 0) c = SMALL;
        else if (f & FLAG_BED) c = BED;
        if (face + u.params.w == u.params.x) c = mix(c, HOVER, 0.65);
        MeshOut out;
        out.position = u.viewProj * (u.model * float4(p, 1));
        out.normal = (u.model * float4(n, 0)).xyz;
        out.color = c;
        return out;
    }

    fragment float4 mesh_fragment(MeshOut in [[stage_in]], bool front [[front_facing]]) {
        if (!front) return float4(BACK, 1);
        float3 n = normalize(in.normal);
        // the web rig (web/ui/scene.js): hemisphere + key + fill + a light from under the plate, because
        // the overhangs this tool exists to show are on the underside
        float3 hemi = mix(float3(0.49, 0.518, 0.573), float3(0.737, 0.824, 1.0), n.z * 0.5 + 0.5) * 0.85;
        float key = max(dot(n, normalize(float3(0.6, -1.0, 1.4))), 0.0) * 0.40;
        float fill = max(dot(n, normalize(float3(-1.0, 0.7, 0.5))), 0.0) * 0.14;
        float under = max(dot(n, normalize(float3(-0.35, 0.55, -1.2))), 0.0) * 0.45;
        float3 lit = in.color * min(hemi + key + fill + under, 1.25);
        return float4(saturate(lit), 1);
    }

    // Supports (fins, pad): print space already (no model matrix), one flat colour per draw call, lit like the
    // part. buffer(3) is the colour.
    vertex MeshOut support_vertex(uint vid [[vertex_id]],
                                  const device float* verts [[buffer(0)]],
                                  constant Uniforms& u [[buffer(2)]],
                                  constant float4& color [[buffer(3)]]) {
        uint o = vid * u.params.y;
        MeshOut out;
        out.position = u.viewProj * float4(verts[o], verts[o + 1], verts[o + 2], 1);
        out.normal = float3(verts[o + 3], verts[o + 4], verts[o + 5]);
        out.color = color.xyz;
        return out;
    }

    // The load arrow: model space (it turns with the part), one flat colour, buffer(3).
    vertex MeshOut hud_vertex(uint vid [[vertex_id]],
                              const device float* verts [[buffer(0)]],
                              constant Uniforms& u [[buffer(2)]],
                              constant float4& color [[buffer(3)]]) {
        uint o = vid * u.params.y;
        MeshOut out;
        out.position = u.viewProj * (u.model * float4(verts[o], verts[o + 1], verts[o + 2], 1));
        out.normal = float3(verts[o + 3], verts[o + 4], verts[o + 5]);
        out.color = color.xyz;
        return out;
    }

    // The fin a removal click would drop: translucent, unlit, over the fins (ui/remove.js hoverFinMaterial).
    fragment float4 support_overlay_fragment(MeshOut in [[stage_in]], constant float4& color [[buffer(0)]]) {
        return color;
    }

    struct IdOut {
        float4 position [[position]];
        uint face [[flat]];
    };

    vertex IdOut id_vertex(uint vid [[vertex_id]],
                           const device float* verts [[buffer(0)]],
                           constant Uniforms& u [[buffer(2)]]) {
        uint o = vid * u.params.y;
        float3 p = float3(verts[o], verts[o + 1], verts[o + 2]);
        IdOut out;
        out.position = u.viewProj * (u.model * float4(p, 1));
        out.face = vid / 3 + u.params.w;
        return out;
    }

    // Face index + 1 (0 = nothing); the picking texture is R32Uint.
    fragment uint id_fragment(IdOut in [[stage_in]]) { return in.face + 1; }

    struct LineOut { float4 position [[position]]; };

    vertex LineOut line_vertex(uint vid [[vertex_id]],
                               const device packed_float3* pos [[buffer(0)]],
                               constant Uniforms& u [[buffer(2)]]) {
        LineOut out;
        out.position = u.viewProj * float4(pos[vid], 1);
        return out;
    }

    // A thick polyline in screen space: six vertices a segment (two triangles), `width` pixels wide.
    struct RingParams { float2 viewport; float width; float pad; };

    vertex LineOut ring_vertex(uint vid [[vertex_id]],
                               const device packed_float3* pos [[buffer(0)]],
                               constant Uniforms& u [[buffer(2)]],
                               constant RingParams& r [[buffer(3)]]) {
        uint seg = vid / 6;
        uint k = vid % 6;
        // two triangles of the quad: (t, side) = (0,-1) (0,+1) (1,-1) | (1,-1) (0,+1) (1,+1)
        const uint tIdx[6] = {0, 0, 1, 1, 0, 1};
        const float sideIdx[6] = {-1, 1, -1, -1, 1, 1};
        float4 c0 = u.viewProj * float4(pos[seg], 1);
        float4 c1 = u.viewProj * float4(pos[seg + 1], 1);
        float2 d = normalize((c1.xy / c1.w - c0.xy / c0.w) * r.viewport + 1e-6);
        float2 n = float2(-d.y, d.x);
        float4 c = tIdx[k] == 0 ? c0 : c1;
        c.xy += n * sideIdx[k] * (r.width / r.viewport) * c.w;
        LineOut out;
        out.position = c;
        return out;
    }

    fragment float4 line_fragment(LineOut in [[stage_in]], constant float4& color [[buffer(0)]]) {
        return color;
    }
    """
}
