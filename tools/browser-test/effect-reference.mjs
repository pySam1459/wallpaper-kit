// Runs the installed original GLSL in a browser. No engine source is bundled.
export function effectReference(input) {
  const ripple = input.effect === 'waterripple';
  const pulse = input.effect === 'pulse';
  const canvas = document.createElement('canvas');
  canvas.width = input.width; canvas.height = input.height;
  const gl = canvas.getContext('webgl', { antialias: false, premultipliedAlpha: false, preserveDrawingBuffer: true });
  if (!gl) throw Error('WebGL is required for the installed-shader reference');
  const helpers = `precision highp float;
#define MODE 0
#define PERSPECTIVE 0
#define SPECULAR 0
#define MASK ${input.masked ? 1 : 0}
#define M_PI 3.14159265359
#define CAST2(x) vec2(x)
#define CAST3(x) vec3(x)
#define CAST4(x) vec4(x)
#define saturate(x) clamp(x,0.0,1.0)
#define AUDIOPROCESSING 0
#define BLENDMODE ${input.combos?.BLENDMODE ?? 9}
#define PULSECOLOR ${input.combos?.PULSECOLOR ?? 1}
#define PULSEALPHA ${input.combos?.PULSEALPHA ?? 0}
#define texSample2D texture2D
#define mul(a,b) ((b)*(a))
vec2 rotateVec2(vec2 v, float a) { return mat2(cos(a),sin(a),-sin(a),cos(a))*v; }
`;
  const compile = (type, source) => {
    const shader = gl.createShader(type);
    // These two straight-line shader profiles need explicit float literals in
    // GLSL ES 1.00. Keep preprocessor integers and scientific notation intact.
    const portable = source.replace(/^\s*#include "common_blending.h"\s*$/gm, input.blending || '')
      .replace(/^\s*#include "common(?:_perspective)?.h"\s*$/gm, '')
      .split('\n').map(line => line.trimStart().startsWith('#') ? line : line.replace(
        /(?<![\w.])\d+(?:\.\d*)?(?:[eE][+-]?\d+)?(?![\w.])/g,
        value => /[.eE]/.test(value) ? value : `${value}.0`)).join('\n');
    gl.shaderSource(shader, helpers + portable);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw Error(gl.getShaderInfoLog(shader));
    return shader;
  };
  const program = gl.createProgram();
  gl.attachShader(program, compile(gl.VERTEX_SHADER, input.vertex));
  gl.attachShader(program, compile(gl.FRAGMENT_SHADER, input.fragment));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw Error(gl.getProgramInfoLog(program));
  gl.useProgram(program);
  const attribute = (name, components, data) => {
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.STATIC_DRAW);
    const location = gl.getAttribLocation(program, name);
    gl.enableVertexAttribArray(location); gl.vertexAttribPointer(location, components, gl.FLOAT, false, 0, 0);
  };
  attribute('a_Position', 3, [-1,1,0, -1,-1,0, 1,1,0, 1,-1,0]);
  attribute('a_TexCoord', 2, [0,0, 0,1, 1,0, 1,1]);
  const bytes = value => Uint8Array.from(atob(value), c => c.charCodeAt(0));
  const texture = (slot, w, h, data, repeat) => {
    gl.activeTexture(gl.TEXTURE0 + slot);
    gl.bindTexture(gl.TEXTURE_2D, gl.createTexture());
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, bytes(data));
    for (const key of [gl.TEXTURE_MIN_FILTER, gl.TEXTURE_MAG_FILTER]) gl.texParameteri(gl.TEXTURE_2D, key, gl.LINEAR);
    for (const key of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T]) gl.texParameteri(gl.TEXTURE_2D, key, repeat ? gl.REPEAT : gl.CLAMP_TO_EDGE);
    gl.uniform1i(gl.getUniformLocation(program, `g_Texture${slot}`), slot);
  };
  const sourceWidth = input.source_width ?? input.width, sourceHeight = input.source_height ?? input.height;
  texture(0, sourceWidth, sourceHeight, input.rgba, false);
  const maskWidth = input.mask_width ?? input.width, maskHeight = input.mask_height ?? input.height;
  if (input.masked) texture(pulse ? 2 : 1, maskWidth, maskHeight, input.mask_rgba, false);
  texture(pulse ? 1 : 2, 8, 8, ripple ? input.normal_rgba : input.noise_rgba, true);
  const defaults = ripple ? { animationspeed: .15, scrollspeed: 0, scale: 1, ratio: 1, ripplestrength: .1, scrolldirection: 0 }
    : { speeduv: 5, strength: .4, phase: .5, power: 1, ratio: .3, scale: .05, scrolldirection: 0 };
  const parameters = { ...defaults, ...input.constants };
  const uniforms = ripple ? { g_AnimationSpeed: 'animationspeed', g_ScrollSpeed: 'scrollspeed', g_Scale: 'scale',
    g_Ratio: 'ratio', g_Strength: 'ripplestrength', g_Direction: 'scrolldirection' }
    : { g_Speed: 'speeduv', g_Strength: 'strength', g_Phase: 'phase', g_Power: 'power', g_Ratio: 'ratio', g_NoiseScale: 'scale', g_Direction: 'scrolldirection' };
  for (const [uniform, key] of Object.entries(uniforms)) {
    gl.uniform1f(gl.getUniformLocation(program, uniform), parameters[key]);
  }
  if (pulse) {
    const p = {speed:3,phase:0,amount:1,noisespeed:.5,noiseamount:0,power:1,bounds:'0 1',tintlow:'1 1 1',tinthigh:'1 1 1',...input.constants};
    for (const [uniform,key] of Object.entries({g_PulseSpeed:'speed',g_PulsePhase:'phase',g_PulseAmount:'amount',g_NoiseSpeed:'noisespeed',g_NoiseAmount:'noiseamount',g_Power:'power'})) {
      gl.uniform1f(gl.getUniformLocation(program,uniform),p[key]);
    }
    const vector = value => typeof value==='string' ? value.split(/\s+/).map(Number) : value;
    gl.uniform2fv(gl.getUniformLocation(program,'g_PulseThresholds'),vector(p.bounds));
    gl.uniform3fv(gl.getUniformLocation(program,'g_TintColor1'),vector(p.tintlow));
    gl.uniform3fv(gl.getUniformLocation(program,'g_TintColor2'),vector(p.tinthigh));
  }
  gl.uniform1f(gl.getUniformLocation(program, 'g_Time'), input.time / 1000);
  gl.uniform4f(gl.getUniformLocation(program, 'g_Texture0Resolution'), sourceWidth, sourceHeight, sourceWidth, sourceHeight);
  if (input.masked) gl.uniform4f(gl.getUniformLocation(program, 'g_Texture1Resolution'), maskWidth, maskHeight, maskWidth, maskHeight);
  if (ripple && input.masked) gl.uniform4f(gl.getUniformLocation(program, 'g_Texture2Resolution'), 8, 8, 8, 8);
  if (pulse && input.masked) gl.uniform4f(gl.getUniformLocation(program, 'g_Texture2Resolution'), maskWidth, maskHeight, maskWidth, maskHeight);
  gl.uniformMatrix4fv(gl.getUniformLocation(program, 'g_ModelViewProjectionMatrix'), false, new Float32Array([1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1]));
  gl.clearColor(0, 0, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT);
  if (!input.raw) gl.enable(gl.BLEND);
  gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  const rgba = new Uint8Array(input.width * input.height * 4);
  gl.readPixels(0, 0, input.width, input.height, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
  if (gl.getError() !== gl.NO_ERROR) throw Error('WebGL reference draw failed');
  const flipped = new Uint8Array(rgba.length);
  const stride = input.width * 4;
  for (let y = 0; y < input.height; y++) flipped.set(rgba.subarray(y * stride, (y + 1) * stride), (input.height - 1 - y) * stride);
  gl.getExtension('WEBGL_lose_context')?.loseContext();
  return [...flipped];
}
