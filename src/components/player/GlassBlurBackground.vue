<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { usePlayerStore } from '../../stores/player'

defineProps<{
  isHovered: boolean
}>()

const playerStore = usePlayerStore()
const canvasRef = ref<HTMLCanvasElement | null>(null)

const vertexSrc = `
attribute vec2 a_position;
void main() {
  gl_Position = vec4(a_position, 0, 1);
}
`

const fragmentSrc = `
precision highp float;
uniform vec2 u_resolution;
uniform float u_time;
uniform vec3 u_vibrant;
uniform vec3 u_muted;
uniform vec3 u_dominant;
uniform vec3 u_base;

vec4 permute(vec4 x){return mod(((x*34.0)+1.0)*x,289.0);}
vec4 taylorInvSqrt(vec4 r){return 1.79284291400159-0.85373472095314*r;}
float snoise(vec3 v){
  const vec2 C=vec2(1.0/6.0,1.0/3.0);
  const vec4 D=vec4(0.0,0.5,1.0,2.0);
  vec3 i=floor(v+dot(v,C.yyy));
  vec3 x0=v-i+dot(i,C.xxx);
  vec3 g=step(x0.yzx,x0.xyz);
  vec3 l=1.0-g;
  vec3 i1=min(g.xyz,l.zxy);
  vec3 i2=max(g.xyz,l.zxy);
  vec3 x1=x0-i1+C.xxx;
  vec3 x2=x0-i2+C.yyy;
  vec3 x3=x0-D.yyy;
  i=mod(i,289.0);
  vec4 p=permute(permute(permute(i.z+vec4(0.0,i1.z,i2.z,1.0))+i.y+vec4(0.0,i1.y,i2.y,1.0))+i.x+vec4(0.0,i1.x,i2.x,1.0));
  float n_=1.0/7.0;
  vec3 ns=n_*D.wyz-D.xzx;
  vec4 j=p-49.0*floor(p*7.0*ns.x);
  vec4 x_=floor(j*ns.z);
  vec4 y_=floor(j-7.0*x_);
  vec4 x=x_*ns.x+ns.yyyy;
  vec4 y=y_*ns.x+ns.yyyy;
  vec4 h=1.0-abs(x)-abs(y);
  vec4 b0=vec4(x.xy,y.xy);
  vec4 b1=vec4(x.zw,y.zw);
  vec4 s0=floor(b0)*2.0+1.0;
  vec4 s1=floor(b1)*2.0+1.0;
  vec4 sh=-step(h,vec4(0.0));
  vec4 a0=b0.xzyw+s0.xzyw*sh.xxyy;
  vec4 a1=b1.xzyw+s1.xzyw*sh.zzww;
  vec3 p0=vec3(a0.xy,h.x);
  vec3 p1=vec3(a0.zw,h.y);
  vec3 p2=vec3(a1.xy,h.z);
  vec3 p3=vec3(a1.zw,h.w);
  vec4 norm=taylorInvSqrt(vec4(dot(p0,p0),dot(p1,p1),dot(p2,p2),dot(p3,p3)));
  p0*=norm.x;p1*=norm.y;p2*=norm.z;p3*=norm.w;
  vec4 m=max(0.6-vec4(dot(x0,x0),dot(x1,x1),dot(x2,x2),dot(x3,x3)),0.0);
  m=m*m;
  return 42.0*dot(m*m,vec4(dot(p0,x0),dot(p1,x1),dot(p2,x2),dot(p3,x3)));
}

void main(){
  vec2 uv=gl_FragCoord.xy/u_resolution;
  float n1=snoise(vec3(uv*0.9,u_time*0.12));
  float n2=snoise(vec3(uv*1.1+17.3,u_time*0.10+6.3));
  float n3=snoise(vec3(uv*0.8+8.7,u_time*0.14+12.7));
  const float SHARP=8.0;
  float e1=exp((n1-0.5)*SHARP);
  float e2=exp((n2-0.5)*SHARP);
  float e3=exp((n3-0.5)*SHARP);
  float total=e1+e2+e3;
  vec3 color=u_vibrant*(e1/total)+u_muted*(e2/total)+u_dominant*(e3/total);
  color=mix(u_base,color,0.88);
  float vig=1.0-length(uv-0.5)*0.7;
  color*=vig;
  gl_FragColor=vec4(color,1.0);
}
`

let gl: WebGLRenderingContext | null = null
let program: WebGLProgram | null = null
let animFrame = 0
let startTime = 0

function hexToVec3(hex: string): [number, number, number] {
  const v = parseInt(hex.replace('#', ''), 16)
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255].map(c => c / 255) as [number, number, number]
}

function luminance(r: number, g: number, b: number): number {
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

function clampLuminance(c: [number, number, number], maxL: number): [number, number, number] {
  const l = luminance(c[0], c[1], c[2])
  if (l <= maxL) return c
  const scale = maxL / l
  return [c[0] * scale, c[1] * scale, c[2] * scale]
}

function lerp3(a: [number, number, number], b: [number, number, number], t: number): [number, number, number] {
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

function getThemeColors() {
  const theme = playerStore.theme
  if (theme?.vibrant && theme?.muted && theme?.dominant) {
    const vibrant = clampLuminance(hexToVec3(theme.vibrant), 0.85)
    const muted = clampLuminance(hexToVec3(theme.muted), 0.85)
    const dominant = clampLuminance(hexToVec3(theme.dominant), 0.85)
    const base: [number, number, number] = [dominant[0] * 0.2, dominant[1] * 0.2, dominant[2] * 0.2]
    return { c1: vibrant, c2: muted, c3: dominant, base }
  }
  return null
}

let curVibrant: [number, number, number] = [0.16, 0.10, 0.24]
let curMuted: [number, number, number] = [0.10, 0.10, 0.18]
let curDominant: [number, number, number] = [0.09, 0.13, 0.24]
let curBase: [number, number, number] = [0.03, 0.02, 0.06]

function resize() {
  if (!canvasRef.value) return
  const dpr = 0.15
  canvasRef.value.width = Math.round(window.innerWidth * dpr)
  canvasRef.value.height = Math.round(window.innerHeight * dpr)
  if (gl) gl.viewport(0, 0, canvasRef.value.width, canvasRef.value.height)
}

function initGL() {
  const canvas = canvasRef.value
  if (!canvas) return
  gl = canvas.getContext('webgl') as WebGLRenderingContext
  if (!gl) return

  const vs = gl.createShader(gl.VERTEX_SHADER)!
  gl.shaderSource(vs, vertexSrc)
  gl.compileShader(vs)

  const fs = gl.createShader(gl.FRAGMENT_SHADER)!
  gl.shaderSource(fs, fragmentSrc)
  gl.compileShader(fs)

  program = gl.createProgram()!
  gl.attachShader(program, vs)
  gl.attachShader(program, fs)
  gl.linkProgram(program)
  gl.useProgram(program)

  const positions = new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1])
  const buf = gl.createBuffer()
  gl.bindBuffer(gl.ARRAY_BUFFER, buf)
  gl.bufferData(gl.ARRAY_BUFFER, positions, gl.STATIC_DRAW)
  const loc = gl.getAttribLocation(program, 'a_position')
  gl.enableVertexAttribArray(loc)
  gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0)

  resize()
  startTime = performance.now()
  render()
}

let lastTime = 0
const FRAME_MS = 33

function render(time = 0) {
  animFrame = requestAnimationFrame(render)
  if (time - lastTime < FRAME_MS) return
  lastTime = time
  if (!gl || !program) return

  const dt = (time - startTime) / 1000

  const colors = getThemeColors()
  if (colors) {
    const f = 1 - Math.exp(-dt * 1.5)
    curVibrant = lerp3(curVibrant, colors.c1, f)
    curMuted = lerp3(curMuted, colors.c2, f)
    curDominant = lerp3(curDominant, colors.c3, f)
    curBase = lerp3(curBase, colors.base, f)
  }

  gl.uniform2f(gl.getUniformLocation(program, 'u_resolution'), canvasRef.value!.width, canvasRef.value!.height)
  gl.uniform1f(gl.getUniformLocation(program, 'u_time'), dt)
  gl.uniform3f(gl.getUniformLocation(program, 'u_vibrant'), curVibrant[0], curVibrant[1], curVibrant[2])
  gl.uniform3f(gl.getUniformLocation(program, 'u_muted'), curMuted[0], curMuted[1], curMuted[2])
  gl.uniform3f(gl.getUniformLocation(program, 'u_dominant'), curDominant[0], curDominant[1], curDominant[2])
  gl.uniform3f(gl.getUniformLocation(program, 'u_base'), curBase[0], curBase[1], curBase[2])
  gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4)
}

onMounted(() => {
  initGL()
  window.addEventListener('resize', resize)
})

onUnmounted(() => {
  cancelAnimationFrame(animFrame)
  window.removeEventListener('resize', resize)
  if (gl) {
    const ext = gl.getExtension('WEBGL_lose_context')
    ext?.loseContext()
  }
})
</script>

<template>
  <canvas
    ref="canvasRef"
    class="absolute bottom-0 left-0 pointer-events-none transition-opacity duration-200 blur-xl"
    :class="isHovered ? 'opacity-100' : 'opacity-0'"
    style="height: 250px; width: 500px"
  />
</template>