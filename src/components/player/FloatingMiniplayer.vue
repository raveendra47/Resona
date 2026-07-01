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
import { formatTime, hexToRgba, getTrackDisplayTitle } from '../../lib/utils'
import Slider from '../Slider.vue'
import MarqueeText from '../MarqueeText.vue'
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
  store.currentTrack?.artist || '',
)

const repeatActive = computed(() =>
  ['all', 'one'].includes(store.repeatMode),
)

const repeatIcon = computed(() =>
  store.repeatMode === 'one' ? Repeat1 : Repeat,
)

async function toggleAlwaysOnTop() {
  alwaysOnTop.value = !alwaysOnTop.value
  // Call Tauri API if available
  try {
    const { appWindow } = await import('@tauri-apps/api/window')
    await appWindow.setAlwaysOnTop(alwaysOnTop.value)
  } catch (e) {
    console.warn('Failed to set always on top:', e)
  }
}

onMounted(async () => {
  // Restore pin state from Tauri
  try {
    const { appWindow } = await import('@tauri-apps/api/window')
    const isAlwaysOnTop = await appWindow.isAlwaysOnTop()
    alwaysOnTop.value = isAlwaysOnTop
  } catch (e) {
    // Not a Tauri window
  }
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

function closeFloatingPlayer() {
  store.isMiniWindowOpen = false
  try {
    const { appWindow } = await import('@tauri-apps/api/window')
    const miniWindow = appWindow.getByLabel('mini')
    if (miniWindow) miniWindow.close()
  } catch (e) {
    console.warn('Failed to close mini window:', e)
  }
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

    <!-- Drag handle for Windows -->
    <div
      v-if="deviceStore.isWindows"
      class="absolute top-0 left-0 right-0 h-10 z-20"
      style="--wails-draggable: drag"
    />

    <!-- WebGL Glass Blur Background -->
    <GlassBlurBackground
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
            :model-value="store.isMuted ? 0 : store.volume * 100"
            :min="0"
            :max="100"
            :step="1"
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
          <VolumeX v-if="store.isMuted" class="w-3.5 h-3.5" />
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