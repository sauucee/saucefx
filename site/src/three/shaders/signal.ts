export const SIGNAL_VERT = /* glsl */ `
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position.xy, 0.0, 1.0); // fullscreen passthrough
}
`;

export const SIGNAL_FRAG = /* glsl */ `
precision highp float;

uniform float uTime;
uniform float uLevel;   // 0..1 engine level
uniform vec3  uAccent;  // --accent as 0..1 rgb
varying vec2 vUv;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123); }
float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
             mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}
float fbm(vec2 p) {
  float v = 0.0;
  float a = 0.5;
  for (int i = 0; i < 4; i++) { v += a * noise(p); p *= 2.03; a *= 0.5; }
  return v;
}

void main() {
  vec2 uv = vUv;
  vec2 q = vec2(uv.x * 3.0, uv.y * 2.0 - uTime * 0.05);
  float field = fbm(q + fbm(q + uTime * 0.03) * 0.6);

  // spectrogram contours: thin accent lines crawling over the field
  float contour = smoothstep(0.06, 0.0, abs(fract(field * 14.0) - 0.5));

  // hot floor: accent energy breathing with the signal
  float floorGlow = pow(1.0 - uv.y, 3.0) * (0.35 + 0.65 * uLevel);
  vec3 metal = vec3(0.027, 0.027, 0.031); // sits on --bg-deep

  vec3 col = metal;
  col += uAccent * contour * (0.10 + 0.20 * uLevel);
  col += uAccent * floorGlow * 0.5;
  col += uAccent * pow(field, 3.0) * 0.14;

  // vignette — the type always wins
  float vig = smoothstep(1.15, 0.35, distance(uv, vec2(0.5, 0.42)));
  col *= mix(0.5, 1.0, vig);

  gl_FragColor = vec4(col, 1.0);
}
`;
