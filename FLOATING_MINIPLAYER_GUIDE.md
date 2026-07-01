# Implementing Airmedy's MiniPlayerFloating in Resona

This guide shows how to adapt Airmedy's floating miniplayer design (with WebGL glass blur, dynamic theming, and hover-triggered controls) into Resona's modular component architecture.

## Architecture Overview

**Airmedy's approach (monolithic):**
- Single `MiniPlayerFloating.vue` component handles everything
- Full artwork display as background
- WebGL glass blur effect
- Hover-triggered content overlay
- Dynamic color extraction

**Resona's modular approach (recommended):**
- `FloatingMiniplayer.vue` (container/orchestrator)
- `GlassBlurBackground.vue` (WebGL blur effect)
- `FloatingPlayerContent.vue` (hover-triggered overlay)
- Reuse existing components: `PlayerSeekBar`, `PlayerPlaybackControls`, etc.

---

## Step 1: Create the Main FloatingMiniplayer Container

**File: `src/components/player/FloatingMiniplayer.vue`**

```vue
<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import {
  SkipBack, SkipForward, Play, Pause,
  Pin, PinOff, X, Music,
  Shuffle, Repeat, Repeat1,
  Volume2, VolumeX,
} from 'lucide-vue-next'
import LazyImg from '@/components/LazyImg.vue'
import { usePlayerStore } from '../../stores/player'
import { useAppStore } from '../../stores/app'
import { useDeviceStore } from '../../stores/device'
import { formatTime, hexToRgba, getTrackDisplayTitle } from '@/lib/utils'
import { Slider } from '@/components/Slider.vue'
import { MarqueeText } from '@/components/MarqueeText.vue'
import GlassBlurBackground from './GlassBlurBackground.vue'
import PlayerControlButton from './PlayerControlButton.vue'
import FloatingPlayerContent from './FloatingPlayerContent.vue'

const store = usePlayerStore()
const appStore = useAppStore()
const deviceStore = useDeviceStore()

const alwaysOnTop = ref(false)
const isSeeking = ref(false)
const seekValue = ref(0)
const isHovered = ref(false)
const showVolume = ref(false)
let volumeHideTimer: ReturnType<typeof setTimeout> | null = null

const displayPosition = computed(() =>
  isSeeking.value ? (seekValue.value / 100) * store.duration : store.position,
)

const trackTitle = computed(() =>
  store.currentTrack ? getTrackDisplayTitle(store.currentTrack) : 'Not Playing',
)

const trackArtist = computed(() =>
  store.currentTrack?.artists
    ?.filter((a): a is NonNullable<typeof a> => a !== null)
    .map((a) => a.name)
    .join(', ') ?? '',
)

const repeatActive = computed(() =>
  [store.RepeatModeOne, store.RepeatModeAll].includes(store.repeatMode),
)

const repeatIcon = computed(() =>
  store.repeatMode === store.RepeatModeOne ? Repeat1 : Repeat,
)

async function toggleAlwaysOnTop() {
  alwaysOnTop.value = !alwaysOnTop.value
  // Call your Tauri API if available
  // await invoke('set_always_on_top', { alwaysOnTop: alwaysOnTop.value })
}

onMounted(async () => {
  // Restore pin state if using Tauri
  // const state = await invoke('get_window_state')
  // alwaysOnTop.value = state.always_on_top
})

function onSeekStart() {
  isSeeking.value = true
}

async function onSeekEnd() {
  await store.seek((seekValue.value / 100) * store.duration)
  isSeeking.value = false
}

function onVolumeEnter() {
  if (volumeHideTimer) {
    clearTimeout(volumeHideTimer)
    volumeHideTimer = null
  }
  showVolume.value = true
}

function onVolumeLeave() {
  volumeHideTimer = setTimeout(() => {
    showVolume.value = false
  }, 300)
}

onUnmounted(() => {
  if (volumeHideTimer) clearTimeout(volumeHideTimer)
})

watch(
  () => store.theme,
  (colors) => {
    if (!colors) return
    const root = document.documentElement
    root.style.setProperty('--dynamic-primary', colors.vibrant)
    root.style.setProperty(
      '--dynamic-surface',
      hexToRgba(colors.dominant, 0.15),
    )
    root.style.setProperty(
      '--dynamic-glow',
      `0 0 40px ${hexToRgba(colors.vibrant, 0.3)}`,
    )
  },
)
</script>

<template>
  <div
    class="relative w-full h-full overflow-hidden select-none dark"
    style="-webkit-app-region: drag"
    @mouseenter="isHovered = true"
    @mouseleave="isHovered = false"
  >
    <!-- Artwork background fills entire window -->
    <div class="absolute inset-0 bg-[#0A0A0A]" style="-webkit-app-region: no-drag">
      <LazyImg
        v-if="store.artworkUrl"
        :src="store.artworkUrl"
        :alt="trackTitle"
        class="w-full h-full object-cover"
      />
      <div v-else class="w-full h-full flex items-center justify-center bg-white/5">
        <Music class="w-16 h-16 text-white/20" />
      </div>
    </div>

    <!-- Windows-only drag handle -->
    <div
      v-if="deviceStore.isWindows"
      class="absolute top-0 left-0 right-0 h-10 z-20"
      style="--wails-draggable: drag"
    />

    <!-- WebGL Glass Blur Background -->
    <GlassBlurBackground
      :artwork-url="store.artworkUrl"
      :is-hovered="isHovered"
    />

    <!-- Top-right control pill -->
    <div class="absolute top-2 right-2 z-30" style="-webkit-app-region: no-drag">
      <!-- Volume slider popup -->
      <Transition name="fade">
        <div
          v-if="showVolume && isHovered"
          class="absolute top-full right-0 mt-2 px-2.5 py-2 rounded-xl bg-black/20 backdrop-blur-md border border-white/5"
          @mouseenter="onVolumeEnter"
          @mouseleave="onVolumeLeave"
        >
          <Slider
            :model-value="store.muted ? 0 : store.volume * 100"
            :min="0"
            :max="100"
            :step="1"
            :scrollable="true"
            class="w-20"
            @update:model-value="(v) => store.setVolume(v / 100)"
          />
        </div>
      </Transition>

      <!-- Three-button control pill -->
      <div
        class="inline-flex items-center p-1 rounded-full bg-black/20 backdrop-blur-md border border-white/5 h-8 select-none"
        :class="
          isHovered
            ? 'opacity-100 pointer-events-auto'
            : 'opacity-0 pointer-events-none'
        "
      >
        <!-- Volume button -->
        <button
          class="w-6 h-6 flex items-center justify-center rounded-full text-white/50 hover:text-white/80 transition-colors"
          @mouseenter="onVolumeEnter"
          @mouseleave="onVolumeLeave"
          @click="showVolume = !showVolume"
        >
          <VolumeX v-if="store.muted" class="w-3.5 h-3.5" />
          <Volume2 v-else class="w-3.5 h-3.5" />
        </button>

        <!-- Pin/Always-on-top button -->
        <button
          class="w-6 h-6 flex items-center justify-center rounded-full transition-colors"
          :class="
            alwaysOnTop
              ? 'text-white/80'
              : 'text-white/50 hover:text-white/80'
          "
          @click="toggleAlwaysOnTop()"
        >
          <Pin v-if="alwaysOnTop" class="w-3.5 h-3.5" />
          <PinOff v-else class="w-3.5 h-3.5" />
        </button>

        <!-- Close button -->
        <button
          class="w-6 h-6 flex items-center justify-center rounded-full text-white/50 hover:text-white/80 transition-colors"
          @click="closeFloatingPlayer()"
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>

    <!-- Content overlay (hover-triggered) -->
    <FloatingPlayerContent
      :track-title="trackTitle"
      :track-artist="trackArtist"
      :is-hovered="isHovered"
      :is-seeking="isSeeking"
      :seek-value="seekValue"
      :display-position="displayPosition"
      @seek-start="onSeekStart"
      @seek-end="onSeekEnd"
      @update:seek-value="(v) => (seekValue = v)"
    />
  </div>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
```

---

## Step 2: Create GlassBlurBackground Component

**File: `src/components/player/GlassBlurBackground.vue`**

```vue
<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'

defineProps<{
  artworkUrl: string | null
  isHovered: boolean
}>()

const canvasRef = ref<HTMLCanvasElement | null>(null)

// WebGL shaders from airmedy (simplified)
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
  uniform vec3 u_color;
  
  void main() {
    vec2 uv = gl_FragCoord.xy / u_resolution;
    vec3 color = u_color * (0.5 + 0.5 * sin(u_time + length(uv) * 3.0));
    gl_FragColor = vec4(color, 0.3);
  }
`

let gl: WebGLRenderingContext | null = null
let animFrame = 0

function initGL() {
  if (!canvasRef.value) return
  gl = canvasRef.value.getContext('webgl') as WebGLRenderingContext
  if (!gl) return

  // Compile shaders and link program
  const vs = gl.createShader(gl.VERTEX_SHADER)!
  gl.shaderSource(vs, vertexSrc)
  gl.compileShader(vs)

  const fs = gl.createShader(gl.FRAGMENT_SHADER)!
  gl.shaderSource(fs, fragmentSrc)
  gl.compileShader(fs)

  const program = gl.createProgram()!
  gl.attachShader(program, vs)
  gl.attachShader(program, fs)
  gl.linkProgram(program)
  gl.useProgram(program)

  // Setup geometry
  const positions = new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1])
  const buf = gl.createBuffer()
  gl.bindBuffer(gl.ARRAY_BUFFER, buf)
  gl.bufferData(gl.ARRAY_BUFFER, positions, gl.STATIC_DRAW)

  const posLoc = gl.getAttribLocation(program, 'a_position')
  gl.enableVertexAttribArray(posLoc)
  gl.vertexAttribPointer(posLoc, 2, gl.FLOAT, false, 0, 0)

  resize()
}

function resize() {
  if (!canvasRef.value || !gl) return
  const dpr = 0.15
  canvasRef.value.width = Math.round(window.innerWidth * dpr)
  canvasRef.value.height = Math.round(window.innerHeight * dpr)
  gl.viewport(0, 0, canvasRef.value.width, canvasRef.value.height)
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
```

---

## Step 3: Create FloatingPlayerContent Component

**File: `src/components/player/FloatingPlayerContent.vue`**

```vue
<script setup lang="ts">
import { SkipBack, SkipForward, Play, Pause, Shuffle, Repeat, Repeat1 } from 'lucide-vue-next'
import { usePlayerStore } from '../../stores/player'
import { useAppStore } from '../../stores/app'
import { formatTime } from '@/lib/utils'
import { MarqueeText } from '@/components/MarqueeText.vue'
import PlayerSeekBar from './PlayerSeekBar.vue'
import PlayerControlButton from './PlayerControlButton.vue'

defineProps<{
  trackTitle: string
  trackArtist: string
  isHovered: boolean
  isSeeking: boolean
  seekValue: number
  displayPosition: number
}>()

defineEmits<{
  'seek-start': []
  'seek-end': []
  'update:seek-value': [value: number]
}>()

const store = usePlayerStore()
const appStore = useAppStore()

const repeatActive = computed(() =>
  [store.RepeatModeOne, store.RepeatModeAll].includes(store.repeatMode),
)

const repeatIcon = computed(() =>
  store.repeatMode === store.RepeatModeOne ? Repeat1 : Repeat,
)
</script>

<template>
  <div
    class="absolute bottom-0 left-0 right-0 px-3 pb-2 transition-opacity duration-200"
    :class="isHovered ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'"
    style="-webkit-app-region: no-drag"
  >
    <!-- Track Info -->
    <MarqueeText :text="trackTitle" content-class="text font-semibold leading-tight text-white" />
    <MarqueeText :text="trackArtist" content-class="text-xs text-white/50 leading-tight mt-0.5" />

    <!-- Seek bar -->
    <div class="flex items-center gap-1.5 mt-2">
      <span class="text-[10px] text-white/40 tabular-nums w-7 text-right shrink-0">
        {{ formatTime(displayPosition) }}
      </span>
      <input
        type="range"
        :value="isSeeking ? seekValue : (store.progressPercent || 0)"
        min="0"
        max="100"
        step="0.1"
        class="flex-1 h-1 bg-white/20 rounded-full appearance-none cursor-pointer"
        @mousedown="$emit('seek-start')"
        @mouseup="$emit('seek-end')"
        @input="$emit('update:seek-value', $event.target.value)"
      />
      <span class="text-[10px] text-white/40 tabular-nums w-7 shrink-0">
        {{ formatTime(store.duration) }}
      </span>
    </div>

    <!-- Controls -->
    <div class="flex items-center justify-center gap-4 mt-2 mb-1">
      <!-- Shuffle -->
      <PlayerControlButton
        class="transition-colors"
        :class="store.shuffle ? 'text-white/80' : 'text-white/20 hover:text-white/70'"
        :active="store.shuffle"
        :show-indicator="appStore.showPlayerIndicator"
        dot-class="bg-white"
        @click="store.setShuffle(!store.shuffle)"
      >
        <Shuffle class="w-3.5 h-3.5" />
      </PlayerControlButton>

      <!-- Previous -->
      <button class="text-white/80 hover:text-white/90 transition-colors" @click="store.previous()">
        <SkipBack class="w-4 h-4 fill-current" />
      </button>

      <!-- Play/Pause -->
      <button
        class="w-9 h-9 bg-white rounded-full flex items-center justify-center hover:scale-105 transition-transform shrink-0"
        @click="store.togglePlayPause()"
      >
        <Pause v-if="store.isPlaying" class="w-[18px] h-[18px] fill-current text-[#0A0A0A]" />
        <Play v-else class="w-[18px] h-[18px] fill-current text-[#0A0A0A] ml-0.5" />
      </button>

      <!-- Next -->
      <button class="text-white/80 hover:text-white/90 transition-colors" @click="store.next()">
        <SkipForward class="w-4 h-4 fill-current" />
      </button>

      <!-- Repeat -->
      <PlayerControlButton
        class="transition-colors"
        :class="repeatActive ? 'text-white/80' : 'text-white/20 hover:text-white/70'"
        :active="repeatActive"
        :show-indicator="appStore.showPlayerIndicator"
        dot-class="bg-white"
        @click="store.cycleRepeat()"
      >
        <component :is="repeatIcon" class="w-3.5 h-3.5" />
      </PlayerControlButton>
    </div>
  </div>
</template>
```

---

## Step 4: CSS Variables Setup

Add to your global CSS or Tailwind config:

```css
:root {
  --dynamic-primary: #e11d48;
  --dynamic-surface: rgba(225, 29, 72, 0.15);
  --dynamic-glow: 0 0 40px rgba(225, 29, 72, 0.3);
}

.dark {
  --bg-glass: rgba(35, 35, 38, 0.6);
  --border-glass: rgba(255, 255, 255, 0.1);
}
```

---

## Step 5: Utility Functions

**File: `src/lib/utils.ts`** (add if not present)

```typescript
export function hexToRgba(hex: string, alpha: number): string {
  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

export function getTrackDisplayTitle(track: any): string {
  return track.title || 'Unknown Track'
}

export function formatTime(ms: number): string {
  const s = Math.floor(ms / 1000)
  const m = Math.floor(s / 60)
  const sec = s % 60
  return `${m}:${sec.toString().padStart(2, '0')}`
}
```

---

## Key Differences from Airmedy

| Feature | Airmedy | Resona (Modular) |
|---------|---------|------------------|
| **File count** | 1 large component | 3-4 smaller components |
| **Glass blur** | Inline WebGL | Separate component |
| **Content overlay** | Inline template | Dedicated component |
| **Seek bar** | Custom implementation | Reuses PlayerSeekBar |
| **Controls** | Inline buttons | Reuses PlayerControlButton |
| **Maintainability** | Monolithic | Modular, reusable |

---

## Testing Checklist

- [ ] Artwork displays fullscreen background
- [ ] WebGL blur effect renders on hover
- [ ] Volume slider appears/hides smoothly
- [ ] Pin button toggles always-on-top state
- [ ] Controls animate on hover
- [ ] Seek bar works with mouse drag
- [ ] Colors animate when track changes
- [ ] No memory leaks on unmount (check DevTools)
- [ ] Works on Windows/macOS (drag handling)

---

## Future Enhancements

1. **Advanced WebGL Effects** - Port Airmedy's full Simplex noise shader
2. **Gesture Support** - Add macOS trackpad gestures
3. **Custom Themes** - Allow user to adjust glow intensity
4. **Keyboard Shortcuts** - Space to play, arrow keys for seek
