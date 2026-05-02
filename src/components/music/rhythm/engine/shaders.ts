/**
 * GLSL sources for the rhythm-game WebGL2 renderer.
 *
 * Three shader programs:
 *   - lanesProgram:  draws the per-pitch lane background gradients
 *   - notesProgram:  instanced rounded-rect notes with hue/glow
 *   - particlesProgram: additive radial particles for hit feedback
 *
 * The particle vertex shader animates instances entirely on the GPU
 * given a `uTime` uniform, so the CPU only sets up new bursts.
 */

export const LANES_VERTEX = /* glsl */ `#version 300 es
in vec2 aPos;
in vec2 aLane; // x: laneCenter (px), y: laneWidth (px)
in vec3 aColor;
in float aIsBlack;
uniform vec2 uViewport;
uniform float uHitLineY;
out vec3 vColor;
out float vY01;
out float vIsBlack;
void main() {
  float halfW = aLane.y * 0.5;
  // Lane spans full canvas height
  vec2 px = vec2(aLane.x + aPos.x * halfW, aPos.y * uViewport.y);
  // Convert to clip
  vec2 ndc = (px / uViewport) * 2.0 - 1.0;
  ndc.y = -ndc.y;
  gl_Position = vec4(ndc, 0.0, 1.0);
  vColor = aColor;
  vY01 = aPos.y;
  vIsBlack = aIsBlack;
}
`;

export const LANES_FRAGMENT = /* glsl */ `#version 300 es
precision highp float;
in vec3 vColor;
in float vY01;
in float vIsBlack;
uniform float uHitLineY01;
uniform float uIntensity;
out vec4 outColor;
void main() {
  float darken = mix(0.0, 0.18, vIsBlack);
  // Soft top-to-bottom gradient with a subtle band near the hit line
  float top = smoothstep(0.0, 0.6, vY01);
  float band = smoothstep(0.06, 0.0, abs(vY01 - uHitLineY01));
  vec3 base = vColor * (0.18 + top * 0.06) - vec3(darken);
  base += vColor * band * 0.35 * uIntensity;
  // Vignette towards lane edges via uv (we'll fake using vY01 alone — keep cheap)
  outColor = vec4(base, 0.95);
}
`;

export const NOTES_VERTEX = /* glsl */ `#version 300 es
in vec2 aQuad;          // unit quad [-0.5..0.5]
in vec2 iCenter;        // px center
in vec2 iSize;          // px size
in vec3 iColor;
in float iGlow;         // 0..1 extra glow
in float iIsHold;       // 0/1
in float iAlpha;        // 0..1 alpha
uniform vec2 uViewport;
out vec2 vUV;
out vec3 vColor;
out float vGlow;
out float vIsHold;
out float vAlpha;
out vec2 vSize;
void main() {
  vec2 px = iCenter + aQuad * iSize;
  vec2 ndc = (px / uViewport) * 2.0 - 1.0;
  ndc.y = -ndc.y;
  gl_Position = vec4(ndc, 0.0, 1.0);
  vUV = aQuad;            // [-0.5..0.5]
  vColor = iColor;
  vGlow = iGlow;
  vIsHold = iIsHold;
  vAlpha = iAlpha;
  vSize = iSize;
}
`;

export const NOTES_FRAGMENT = /* glsl */ `#version 300 es
precision highp float;
in vec2 vUV;
in vec3 vColor;
in float vGlow;
in float vIsHold;
in float vAlpha;
in vec2 vSize;
out vec4 outColor;

// Signed distance from the nearest edge of a rounded rectangle.
float sdRoundRect(vec2 p, vec2 b, float r) {
  vec2 q = abs(p) - b + r;
  return length(max(q, 0.0)) - r;
}

void main() {
  vec2 halfSize = vSize * 0.5;
  vec2 px = vUV * vSize;
  float radius = min(min(halfSize.x, halfSize.y) * 0.55, vIsHold > 0.5 ? halfSize.x * 0.85 : 8.0);
  float d = sdRoundRect(px, halfSize, radius);
  // Anti-aliased fill
  float fill = 1.0 - smoothstep(-1.0, 1.0, d);
  if (fill <= 0.001) discard;
  // Top-edge sheen + bottom-edge highlight near hit line
  float topShine = smoothstep(0.45, 0.50, vUV.y) * 0.35;
  vec3 col = vColor;
  // Inner glow grows with vGlow
  float inner = smoothstep(8.0, -10.0, d);
  col += vec3(0.10) + vColor * inner * 0.45;
  col += vec3(topShine);
  // Subtle outline
  float outline = smoothstep(0.0, -2.0, d);
  col = mix(col * 0.7, col, outline);
  outColor = vec4(col, fill * vAlpha);
}
`;

export const PARTICLES_VERTEX = /* glsl */ `#version 300 es
in vec2 aQuad;         // unit quad
in vec2 iOrigin;       // px
in vec2 iVelocity;     // px/s
in vec3 iColor;
in float iBirth;       // seconds
in float iLifetime;    // seconds
in float iSize;        // px radius
uniform vec2 uViewport;
uniform float uTime;   // seconds
out vec3 vColor;
out vec2 vUV;
out float vAge01;
void main() {
  float age = uTime - iBirth;
  float t = clamp(age / iLifetime, 0.0, 1.0);
  // Ease-out trajectory with mild gravity
  vec2 pos = iOrigin + iVelocity * age - vec2(0.0, 80.0) * age * age * 0.5;
  float scale = mix(1.0, 0.2, t);
  vec2 px = pos + aQuad * iSize * scale;
  vec2 ndc = (px / uViewport) * 2.0 - 1.0;
  ndc.y = -ndc.y;
  gl_Position = vec4(ndc, 0.0, 1.0);
  vColor = iColor;
  vUV = aQuad;
  vAge01 = t;
}
`;

export const PARTICLES_FRAGMENT = /* glsl */ `#version 300 es
precision highp float;
in vec3 vColor;
in vec2 vUV;
in float vAge01;
out vec4 outColor;
void main() {
  float r = length(vUV) * 2.0;
  if (r > 1.0) discard;
  float falloff = pow(1.0 - r, 2.0);
  float alpha = falloff * (1.0 - vAge01);
  outColor = vec4(vColor * (0.6 + falloff * 0.6), alpha);
}
`;

export const HIT_LINE_VERTEX = /* glsl */ `#version 300 es
in vec2 aPos;
uniform vec2 uViewport;
uniform float uHitLineY;
uniform float uHeight;
out float vY01;
void main() {
  float y = uHitLineY + aPos.y * uHeight * 0.5;
  vec2 px = vec2(aPos.x * uViewport.x * 0.5 + uViewport.x * 0.5, y);
  vec2 ndc = (px / uViewport) * 2.0 - 1.0;
  ndc.y = -ndc.y;
  gl_Position = vec4(ndc, 0.0, 1.0);
  vY01 = aPos.y;
}
`;

export const HIT_LINE_FRAGMENT = /* glsl */ `#version 300 es
precision highp float;
in float vY01;
uniform vec3 uColor;
uniform float uPulse; // 0..1
out vec4 outColor;
void main() {
  float core = smoothstep(0.85, 1.0, 1.0 - abs(vY01));
  float glow = smoothstep(0.0, 1.0, 1.0 - abs(vY01)) * 0.4;
  float a = core + glow * (0.35 + uPulse * 0.65);
  outColor = vec4(uColor, a);
}
`;
