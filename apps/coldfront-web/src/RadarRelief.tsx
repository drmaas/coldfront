import { useEffect, useRef } from "react";
import {
  buildEchoField,
  DEFAULT_COLS,
  DEFAULT_ROWS,
  fitOrthoHalfExtents,
  type EchoSample,
} from "./radarRelief";

export interface RadarReliefProps {
  echoes: EchoSample[];
  cols?: number;
  rows?: number;
  /** Table under the heightfield: basemap or plain fill, plus pin. No Flat radar. */
  ground: ImageData | null;
  empty: boolean;
  onRecenterRef?: (recenter: () => void) => void;
}

/** Match the Flat stage so a gap does not read as a different map. */
const CLEAR = [213 / 255, 227 / 255, 236 / 255, 1] as const;
/** Near top-down from the south — toggles like Flat, height still pops. */
const DEFAULT_YAW = 0;
const DEFAULT_PITCH = 1.32;
const MIN_DISTANCE = 1.6;
const MAX_DISTANCE = 7;
const MAX_HEIGHT = 0.55;
const GROUND_DEPTH = 1.85;

export function RadarRelief({
  echoes,
  cols = DEFAULT_COLS,
  rows = DEFAULT_ROWS,
  ground,
  empty,
  onRecenterRef,
}: RadarReliefProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drag = useRef<{ x: number; y: number; yaw: number; pitch: number } | null>(null);
  const camera = useRef({
    yaw: DEFAULT_YAW,
    pitch: DEFAULT_PITCH,
    distance: fitDistance(GROUND_DEPTH * (4 / 3), GROUND_DEPTH),
  });
  const echoesRef = useRef(echoes);
  const gridRef = useRef({ cols, rows });
  const groundRef = useRef(ground);
  echoesRef.current = echoes;
  gridRef.current = { cols, rows };
  groundRef.current = ground;

  useEffect(() => {
    if (!onRecenterRef) return;
    onRecenterRef(() => {
      const extent = groundExtent(groundRef.current);
      camera.current = {
        yaw: DEFAULT_YAW,
        pitch: DEFAULT_PITCH,
        distance: fitDistance(extent.width, extent.depth),
      };
    });
  }, [onRecenterRef]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    const gl = canvas.getContext("webgl2", { alpha: false, antialias: true, depth: true });
    if (!gl) return undefined;

    const program = buildProgram(gl);
    if (!program) return undefined;

    const groundMesh = makeGround(gl);
    const slabMesh = makeSlab(gl);
    const mastMesh = makeMast(gl);
    const fieldMesh = makeEmptyField(gl);
    const locs = locations(gl, program);
    let groundTexture: WebGLTexture | null = null;
    let lastGround: ImageData | null = null;
    let lastEchoes: EchoSample[] | null = null;
    let cancelled = false;
    let frame = 0;

    const resize = () => {
      const ratio = 2;
      const width = canvas.clientWidth;
      const height = canvas.clientHeight;
      if (width === 0 || height === 0) return;
      const nextW = Math.round(width * ratio);
      const nextH = Math.round(height * ratio);
      if (canvas.width !== nextW || canvas.height !== nextH) {
        canvas.width = nextW;
        canvas.height = nextH;
      }
      gl.viewport(0, 0, canvas.width, canvas.height);
    };

    const draw = () => {
      if (cancelled) return;
      resize();
      const aspect = canvas.clientWidth / Math.max(1, canvas.clientHeight);
      const cam = camera.current;
      const extent = groundExtent(groundRef.current);
      const locked =
        Math.abs(cam.yaw - DEFAULT_YAW) < 0.001 && Math.abs(cam.pitch - DEFAULT_PITCH) < 0.001;
      const view = lookAt(cam.yaw, cam.pitch, cam.distance);
      const projection = orthoForBoard(view, extent, aspect, cam.distance, locked);
      const vp = multiply(projection, view);
      const currentGround = groundRef.current;
      if (currentGround && currentGround !== lastGround) {
        const prevAspect = lastGround && lastGround.height > 0 ? lastGround.width / lastGround.height : 0;
        const nextAspect = currentGround.height > 0 ? currentGround.width / currentGround.height : 0;
        if (groundTexture) gl.deleteTexture(groundTexture);
        groundTexture = uploadTexture(gl, currentGround);
        lastGround = currentGround;
        if (Math.abs(prevAspect - nextAspect) > 0.02) {
          camera.current.distance = clamp(fitDistance(extent.width, extent.depth), MIN_DISTANCE, MAX_DISTANCE);
        }
      }

      const currentEchoes = echoesRef.current;
      if (currentEchoes !== lastEchoes) {
        uploadField(
          gl,
          fieldMesh,
          buildEchoField(currentEchoes, gridRef.current.cols, gridRef.current.rows),
          extent,
        );
        lastEchoes = currentEchoes;
      }

      gl.enable(gl.DEPTH_TEST);
      gl.clearColor(CLEAR[0], CLEAR[1], CLEAR[2], CLEAR[3]);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.useProgram(program);
      gl.uniformMatrix4fv(locs.uViewProjection, false, vp);
      gl.uniform2f(locs.uExtent, extent.width, extent.depth);
      gl.uniform1f(locs.uPresent, locked ? 1 : 0);
      gl.uniform3f(locs.uLightDir, -0.48, 0.78, -0.32);

      if (groundTexture) {
        gl.disable(gl.BLEND);
        gl.uniform1i(locs.uMode, 0);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, groundTexture);
        gl.uniform1i(locs.uTexture, 0);
        drawMesh(gl, groundMesh);
        if (!locked) {
          gl.uniform1i(locs.uMode, 3);
          drawMesh(gl, slabMesh);
        }
      }

      gl.disable(gl.BLEND);
      gl.uniform1i(locs.uMode, 1);
      drawMesh(gl, fieldMesh);

      if (!locked) {
        gl.uniform1i(locs.uMode, 2);
        drawMesh(gl, mastMesh);
      }

      frame = window.requestAnimationFrame(draw);
    };

    frame = window.requestAnimationFrame(draw);
    return () => {
      cancelled = true;
      window.cancelAnimationFrame(frame);
      gl.deleteProgram(program);
      deleteMesh(gl, groundMesh);
      deleteMesh(gl, slabMesh);
      deleteMesh(gl, mastMesh);
      deleteMesh(gl, fieldMesh);
      if (groundTexture) gl.deleteTexture(groundTexture);
    };
  }, []);

  return (
    <>
      <canvas
        ref={canvasRef}
        className="radar-relief"
        aria-hidden={empty ? undefined : true}
        onContextMenu={(event) => event.preventDefault()}
        onPointerDown={(event) => {
          // Left-drag pans the map (stage). Shift or right-drag orbits the camera.
          if (event.button !== 2 && !event.shiftKey) return;
          event.stopPropagation();
          drag.current = {
            x: event.clientX,
            y: event.clientY,
            yaw: camera.current.yaw,
            pitch: camera.current.pitch,
          };
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          const startDrag = drag.current;
          if (!startDrag) return;
          camera.current.yaw = startDrag.yaw + (event.clientX - startDrag.x) * 0.005;
          camera.current.pitch = clamp(startDrag.pitch + (event.clientY - startDrag.y) * 0.004, 0.55, 1.45);
        }}
        onPointerUp={() => {
          drag.current = null;
        }}
        onPointerCancel={() => {
          drag.current = null;
        }}
      />
      {empty && <p className="radar-relief-empty">No echoes in this view.</p>}
    </>
  );
}

function groundExtent(ground: ImageData | null): { width: number; depth: number } {
  const aspect = ground && ground.height > 0 ? ground.width / ground.height : 4 / 3;
  return { width: GROUND_DEPTH * aspect, depth: GROUND_DEPTH };
}

function fitDistance(width: number, depth: number): number {
  const halfW = width / 2;
  const halfD = depth / 2;
  return clamp(Math.max(halfW * 2.2, halfD * 2.4), 2.5, MAX_DISTANCE);
}

interface Mesh {
  vao: WebGLVertexArrayObject;
  count: number;
  buffer: WebGLBuffer;
}

interface Locs {
  uViewProjection: WebGLUniformLocation | null;
  uExtent: WebGLUniformLocation | null;
  uMode: WebGLUniformLocation | null;
  uTexture: WebGLUniformLocation | null;
  uLightDir: WebGLUniformLocation | null;
  uPresent: WebGLUniformLocation | null;
}

function locations(gl: WebGL2RenderingContext, program: WebGLProgram): Locs {
  return {
    uViewProjection: gl.getUniformLocation(program, "uViewProjection"),
    uExtent: gl.getUniformLocation(program, "uExtent"),
    uMode: gl.getUniformLocation(program, "uMode"),
    uTexture: gl.getUniformLocation(program, "uTexture"),
    uLightDir: gl.getUniformLocation(program, "uLightDir"),
    uPresent: gl.getUniformLocation(program, "uPresent"),
  };
}

function buildProgram(gl: WebGL2RenderingContext): WebGLProgram | null {
  const vs = `#version 300 es
precision highp float;
layout(location=0) in vec3 aPosition;
layout(location=1) in vec3 aNormal;
layout(location=2) in vec2 aUv;
layout(location=3) in vec3 aColor;
uniform mat4 uViewProjection;
uniform vec2 uExtent;
uniform int uMode;
uniform vec3 uLightDir;
uniform float uPresent;
out vec3 vColor;
out vec2 vUv;
out float vMode;
out float vAlpha;
out float vWall;
void main() {
  vUv = aUv;
  vMode = float(uMode);
  vec3 pos = aPosition;
  vec3 color = aColor;
  float alpha = 1.0;
  float ambient = 0.48;
  float diffuseGain = 0.52;
  if (uMode == 0) {
    pos = vec3((aPosition.x - 0.5) * uExtent.x, 0.0, (aPosition.z - 0.5) * uExtent.y);
    color = vec3(1.0);
    ambient = 1.0;
    diffuseGain = 0.0;
  } else if (uMode == 1) {
    pos = aPosition;
    float peak = clamp(aPosition.y / 0.55, 0.0, 1.0);
    color = aColor * mix(0.72, 1.22, peak);
    alpha = aPosition.y > 0.004 ? 1.0 : 0.0;
    ambient = 0.62;
    diffuseGain = 0.4;
  } else if (uMode == 4) {
    pos = vec3(aPosition.x, 0.002, aPosition.z);
    color = vec3(0.18, 0.26, 0.32);
    ambient = 1.0;
    diffuseGain = 0.0;
    alpha = aPosition.y > 0.02 ? 1.0 : 0.0;
  } else if (uMode == 3) {
    // Thin map slab sides — reads as a physical board, not a floating texture.
    float thick = 0.045;
    pos = vec3((aPosition.x - 0.5) * uExtent.x, aPosition.y * thick - thick, (aPosition.z - 0.5) * uExtent.y);
    color = mix(vec3(0.42, 0.48, 0.52), vec3(0.28, 0.32, 0.36), aPosition.y);
    ambient = 0.35;
    diffuseGain = 0.55;
  } else {
    float mastH = 0.18;
    pos = vec3((aPosition.x - 0.5) * 0.016, aPosition.y * mastH + 0.001, (aPosition.z - 0.5) * 0.016);
    color = aPosition.y > 0.85 ? vec3(1.0) : vec3(0.88, 0.12, 0.3);
  }
  vec3 normal = normalize(aNormal);
  vec3 light = normalize(uLightDir);
  float ndotl = max(0.0, dot(normal, light));
  if (uMode == 1) ndotl = ndotl * 0.35 + 0.65;
  float diffuse = diffuseGain * ndotl;
  vColor = color * (ambient + diffuse);
  vAlpha = alpha;
  vWall = aNormal.y < 0.5 ? 1.0 : 0.0;
  gl_Position = uViewProjection * vec4(pos, 1.0);
}`;

  const fs = `#version 300 es
precision highp float;
in vec3 vColor;
in vec2 vUv;
in float vMode;
in float vAlpha;
in float vWall;
uniform sampler2D uTexture;
uniform float uPresent;
out vec4 outColor;
void main() {
  if (vMode > 3.5 && vWall > 0.5) discard;
  vec3 base = vMode < 0.5 ? texture(uTexture, vUv).rgb : vColor;
  float alpha = (vMode > 0.5 && vMode < 1.5) || vMode > 3.5 ? vAlpha : 1.0;
  if (alpha < 0.5) discard;
  outColor = vec4(base, 1.0);
}`;

  const vertex = compile(gl, gl.VERTEX_SHADER, vs);
  const fragment = compile(gl, gl.FRAGMENT_SHADER, fs);
  if (!vertex || !fragment) return null;
  const program = gl.createProgram();
  if (!program) return null;
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  gl.deleteShader(vertex);
  gl.deleteShader(fragment);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    gl.deleteProgram(program);
    return null;
  }
  return program;
}

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader | null {
  const shader = gl.createShader(type);
  if (!shader) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}

function makeGround(gl: WebGL2RenderingContext): Mesh {
  // Unit quad: z=0 → image top (north, UV v=0), z=1 → image bottom (south).
  // texImage2D without FLIP_Y keeps ImageData row 0 at V=0.
  const data = new Float32Array([
    0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1,
    1, 0, 0, 0, 1, 0, 1, 0, 1, 1, 1,
    1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1,
    0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1,
    1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1,
    0, 0, 1, 0, 1, 0, 0, 1, 1, 1, 1,
  ]);
  return meshFrom(gl, data, 6);
}

/** Unit cube sides (y=0 bottom, y=1 top) stretched by the shader into a thin map slab. */
function makeSlab(gl: WebGL2RenderingContext): Mesh {
  const faces: number[] = [];
  const push = (
    ax: number, ay: number, az: number,
    bx: number, by: number, bz: number,
    cx: number, cy: number, cz: number,
    dx: number, dy: number, dz: number,
    nx: number, ny: number, nz: number,
  ) => {
    const color = [0.35, 0.4, 0.44];
    faces.push(ax, ay, az, nx, ny, nz, 0, 0, ...color);
    faces.push(bx, by, bz, nx, ny, nz, 1, 0, ...color);
    faces.push(cx, cy, cz, nx, ny, nz, 1, 1, ...color);
    faces.push(ax, ay, az, nx, ny, nz, 0, 0, ...color);
    faces.push(cx, cy, cz, nx, ny, nz, 1, 1, ...color);
    faces.push(dx, dy, dz, nx, ny, nz, 0, 1, ...color);
  };
  push(0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, -1);
  push(0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 1);
  push(0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, -1, 0, 0);
  push(1, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0);
  push(0, 0, 0, 0, 0, 1, 1, 0, 1, 1, 0, 0, 0, -1, 0);
  return meshFrom(gl, new Float32Array(faces), faces.length / 11);
}

function makeMast(gl: WebGL2RenderingContext): Mesh {
  const faces: number[] = [];
  const push = (
    ax: number, ay: number, az: number,
    bx: number, by: number, bz: number,
    cx: number, cy: number, cz: number,
    dx: number, dy: number, dz: number,
    nx: number, ny: number, nz: number,
  ) => {
    const color = [0.88, 0.12, 0.3];
    faces.push(ax, ay, az, nx, ny, nz, 0, 0, ...color);
    faces.push(bx, by, bz, nx, ny, nz, 1, 0, ...color);
    faces.push(cx, cy, cz, nx, ny, nz, 1, 1, ...color);
    faces.push(ax, ay, az, nx, ny, nz, 0, 0, ...color);
    faces.push(cx, cy, cz, nx, ny, nz, 1, 1, ...color);
    faces.push(dx, dy, dz, nx, ny, nz, 0, 1, ...color);
  };
  push(0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, -1);
  push(0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 1);
  push(0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, -1, 0, 0);
  push(1, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0);
  push(0, 1, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 0, 1, 0);
  push(0, 0, 0, 0, 0, 1, 1, 0, 1, 1, 0, 0, 0, -1, 0);
  return meshFrom(gl, new Float32Array(faces), faces.length / 11);
}

function makeEmptyField(gl: WebGL2RenderingContext): Mesh {
  return meshFrom(gl, new Float32Array(11 * 3), 0);
}

function uploadField(
  gl: WebGL2RenderingContext,
  mesh: Mesh,
  field: ReturnType<typeof buildEchoField>,
  extent: { width: number; depth: number },
): void {
  const cols = field.cols;
  const rows = field.rows;
  const verts: number[] = [];
  const heightAt = (c: number, r: number) => field.height[r * cols + c] ?? 0;
  const colorAt = (c: number, r: number) => {
    const o = (r * cols + c) * 3;
    return [field.color[o] ?? 0, field.color[o + 1] ?? 0, field.color[o + 2] ?? 0] as const;
  };
  // One height per grid corner, shared by every quad that touches it.
  // Separate per-quad corners left hairline gaps, and the ground showed through as stripes.
  const vcols = cols + 1;
  const vrows = rows + 1;
  const vertexHeight = new Float32Array(vcols * vrows);
  const vertexWeight = new Float32Array(vcols * vrows);
  const vindex = (c: number, r: number) => r * vcols + c;
  for (let r = 0; r < rows; r += 1) {
    for (let c = 0; c < cols; c += 1) {
      const h = heightAt(c, r);
      if (h <= 0.012) continue;
      const y = h * MAX_HEIGHT;
      for (let dr = 0; dr <= 1; dr += 1) {
        for (let dc = 0; dc <= 1; dc += 1) {
          const i = vindex(c + dc, r + dr);
          vertexHeight[i] = (vertexHeight[i] ?? 0) + y;
          vertexWeight[i] = (vertexWeight[i] ?? 0) + 1;
        }
      }
    }
  }
  for (let i = 0; i < vertexHeight.length; i += 1) {
    const w = vertexWeight[i] ?? 0;
    if (w > 0) vertexHeight[i] = (vertexHeight[i] ?? 0) / w;
  }
  const sampleHeight = (c: number, r: number, fallback: number) => {
    if (c < 0 || r < 0 || c >= vcols || r >= vrows) return fallback;
    if ((vertexWeight[vindex(c, r)] ?? 0) <= 0) return fallback;
    return vertexHeight[vindex(c, r)] ?? fallback;
  };
  const dx = extent.width / cols;
  const dz = extent.depth / rows;
  const normalAt = (c: number, r: number): readonly [number, number, number] => {
    const y = sampleHeight(c, r, 0);
    const nx = (sampleHeight(c - 1, r, y) - sampleHeight(c + 1, r, y)) / (2 * dx);
    const nz = (sampleHeight(c, r - 1, y) - sampleHeight(c, r + 1, y)) / (2 * dz);
    const len = Math.hypot(nx, 1, nz) || 1;
    return [nx / len, 1 / len, nz / len];
  };
  const corner = (c: number, r: number) => {
    const u = c / cols;
    const v = r / rows;
    return {
      x: (u - 0.5) * extent.width,
      y: sampleHeight(c, r, 0),
      z: (v - 0.5) * extent.depth,
      u,
      v,
      n: normalAt(c, r),
    };
  };
  const push = (p: ReturnType<typeof corner>, col: readonly [number, number, number]) => {
    verts.push(p.x, p.y, p.z, p.n[0], p.n[1], p.n[2], p.u, p.v, col[0], col[1], col[2]);
  };
  for (let r = 0; r < rows; r += 1) {
    for (let c = 0; c < cols; c += 1) {
      if (heightAt(c, r) <= 0.012) continue;
      const col = colorAt(c, r);
      const p00 = corner(c, r);
      const p10 = corner(c + 1, r);
      const p11 = corner(c + 1, r + 1);
      const p01 = corner(c, r + 1);
      push(p00, col);
      push(p10, col);
      push(p11, col);
      push(p00, col);
      push(p11, col);
      push(p01, col);
    }
  }

  const data = new Float32Array(verts);
  gl.bindVertexArray(mesh.vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, mesh.buffer);
  gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
  bindFieldAttribs(gl);
  gl.bindVertexArray(null);
  mesh.count = data.length / 11;
}

function bindFieldAttribs(gl: WebGL2RenderingContext) {
  const stride = 11 * 4;
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 3, gl.FLOAT, false, stride, 0);
  gl.enableVertexAttribArray(1);
  gl.vertexAttribPointer(1, 3, gl.FLOAT, false, stride, 12);
  gl.enableVertexAttribArray(2);
  gl.vertexAttribPointer(2, 2, gl.FLOAT, false, stride, 24);
  gl.enableVertexAttribArray(3);
  gl.vertexAttribPointer(3, 3, gl.FLOAT, false, stride, 32);
}

function meshFrom(gl: WebGL2RenderingContext, data: Float32Array, count: number): Mesh {
  const vao = gl.createVertexArray();
  const buffer = gl.createBuffer();
  if (!vao || !buffer) throw new Error("webgl mesh");
  gl.bindVertexArray(vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
  bindFieldAttribs(gl);
  gl.bindVertexArray(null);
  return { vao, count, buffer };
}

function drawMesh(gl: WebGL2RenderingContext, mesh: Mesh) {
  if (mesh.count <= 0) return;
  gl.bindVertexArray(mesh.vao);
  gl.drawArrays(gl.TRIANGLES, 0, mesh.count);
  gl.bindVertexArray(null);
}

function deleteMesh(gl: WebGL2RenderingContext, mesh: Mesh) {
  gl.deleteVertexArray(mesh.vao);
  gl.deleteBuffer(mesh.buffer);
}

function uploadTexture(gl: WebGL2RenderingContext, image: ImageData): WebGLTexture | null {
  const texture = gl.createTexture();
  if (!texture) return null;
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, image.width, image.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, image.data);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  return texture;
}

function lookAt(yaw: number, pitch: number, distance: number): Float32Array {
  const cx = Math.sin(yaw) * Math.cos(pitch) * distance;
  const cy = Math.sin(pitch) * distance;
  const cz = Math.cos(yaw) * Math.cos(pitch) * distance;
  return lookAtMatrix(cx, cy, cz, 0, 0, 0, 0, 1, 0);
}

/** Fit an orthographic frustum around the board so edges stay parallel (no trapezoid skew). */
function orthoForBoard(
  view: Float32Array,
  extent: { width: number; depth: number },
  aspect: number,
  distance: number,
  locked: boolean,
): Float32Array {
  const hw = extent.width / 2;
  const hd = extent.depth / 2;
  // Locked view frames the ground itself, so Relief covers the same rectangle as Flat.
  // Orbit keeps headroom for the tallest echoes and follows camera distance.
  const corners = locked
    ? [
        [-hw, 0, -hd],
        [hw, 0, -hd],
        [-hw, 0, hd],
        [hw, 0, hd],
      ]
    : [
        [-hw, 0, -hd],
        [hw, 0, -hd],
        [-hw, 0, hd],
        [hw, 0, hd],
        [-hw, MAX_HEIGHT, -hd],
        [hw, MAX_HEIGHT, hd],
      ];
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const [x, y, z] of corners) {
    const px = (view[0]! * x!) + (view[4]! * y!) + (view[8]! * z!) + view[12]!;
    const py = (view[1]! * x!) + (view[5]! * y!) + (view[9]! * z!) + view[13]!;
    minX = Math.min(minX, px);
    maxX = Math.max(maxX, px);
    minY = Math.min(minY, py);
    maxY = Math.max(maxY, py);
  }
  const dolly = locked ? 1 : clamp(distance / 2.9, 0.75, 1.45);
  // Locked: overscan + cover so the tilted board reaches every stage edge.
  // Contain left a CLEAR (#d5e3ec) letterbox that read as a gray overlay with light seams.
  const pad = locked ? 0.96 : 1.02;
  const fitted = fitOrthoHalfExtents(
    ((maxX - minX) / 2) * pad * dolly,
    ((maxY - minY) / 2) * pad * dolly,
    aspect,
    locked ? "cover" : "contain",
  );
  const near = 0.05;
  const far = 40;
  return new Float32Array([
    1 / fitted.halfW, 0, 0, 0,
    0, 1 / fitted.halfH, 0, 0,
    0, 0, -2 / (far - near), 0,
    0, 0, -(far + near) / (far - near), 1,
  ]);
}

function lookAtMatrix(
  eyex: number, eyey: number, eyez: number,
  centerx: number, centery: number, centerz: number,
  upx: number, upy: number, upz: number,
): Float32Array {
  let zx = eyex - centerx;
  let zy = eyey - centery;
  let zz = eyez - centerz;
  let len = 1 / Math.hypot(zx, zy, zz);
  zx *= len;
  zy *= len;
  zz *= len;
  let xx = upy * zz - upz * zy;
  let xy = upz * zx - upx * zz;
  let xz = upx * zy - upy * zx;
  len = Math.hypot(xx, xy, xz);
  if (len < 1e-6) {
    xx = 0;
    xy = 0;
    xz = 0;
  } else {
    len = 1 / len;
    xx *= len;
    xy *= len;
    xz *= len;
  }
  const yx = zy * xz - zz * xy;
  const yy = zz * xx - zx * xz;
  const yz = zx * xy - zy * xx;
  return new Float32Array([
    xx, yx, zx, 0,
    xy, yy, zy, 0,
    xz, yz, zz, 0,
    -(xx * eyex + xy * eyey + xz * eyez),
    -(yx * eyex + yy * eyey + yz * eyez),
    -(zx * eyex + zy * eyey + zz * eyez),
    1,
  ]);
}

function multiply(a: Float32Array, b: Float32Array): Float32Array {
  const out = new Float32Array(16);
  for (let col = 0; col < 4; col += 1) {
    for (let row = 0; row < 4; row += 1) {
      out[col * 4 + row] =
        (a[row] ?? 0) * (b[col * 4] ?? 0) +
        (a[4 + row] ?? 0) * (b[col * 4 + 1] ?? 0) +
        (a[8 + row] ?? 0) * (b[col * 4 + 2] ?? 0) +
        (a[12 + row] ?? 0) * (b[col * 4 + 3] ?? 0);
    }
  }
  return out;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
