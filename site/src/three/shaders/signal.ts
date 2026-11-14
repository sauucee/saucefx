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
uniform vec3  uMetal;   // --bg-deep as 0..1 rgb (theme-aware base)
uniform vec2  uMouse;   // 0..1 pointer, for parallax drift
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
  vec2 drift = (uMouse - 0.5) * 0.12;
  vec2 q = vec2(uv.x * 3.0 + drift.x, uv.y * 2.0 + drift.y - uTime * 0.02);
  float field = fbm(q + fbm(q + uTime * 0.015) * 0.6);

  // atmosphere only — the signal line is the focal element
  vec3 col = uMetal * (0.97 + 0.06 * field);
  col += uAccent * pow(field, 3.0) * 0.04;

  // vignette — deep void, focus center (lighter on light themes)
  float isLight = step(0.5, dot(uMetal, vec3(0.333)));
  float vigFloor = mix(0.34, 0.88, isLight);
  float vig = smoothstep(1.0, 0.28, distance(uv, vec2(0.5, 0.4)));
  col *= mix(vigFloor, 1.0, vig);

  gl_FragColor = vec4(col, 1.0);
}
`;
