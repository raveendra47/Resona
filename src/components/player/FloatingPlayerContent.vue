<script setup lang="ts">
import { computed } from 'vue'
import { SkipBack, SkipForward, Play, Pause, Shuffle, Repeat, Repeat1 } from 'lucide-vue-next'
import { usePlayerStore } from '../../stores/player'
import { useAppStore } from '../../stores/app'
import { formatTime } from '../../lib/utils'
import MarqueeText from '../MarqueeText.vue'
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
  ['all', 'one'].includes(store.repeatMode),
)

const repeatIcon = computed(() =>
  store.repeatMode === 'one' ? Repeat1 : Repeat,
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
        class="flex-1 h-1 bg-white/20 rounded-full appearance-none cursor-pointer accent-white/80"
        @mousedown="$emit('seek-start')"
        @mouseup="$emit('seek-end')"
        @input="$emit('update:seek-value', Number(($event.target as HTMLInputElement).value))"
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

<style scoped>
input[type='range'] {
  -webkit-appearance: none;
  appearance: none;
  background: transparent;
  cursor: pointer;
}

input[type='range']::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: white;
  cursor: pointer;
  opacity: 0.8;
  transition: opacity 0.2s;
}

input[type='range']::-webkit-slider-thumb:hover {
  opacity: 1;
}

input[type='range']::-moz-range-thumb {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: white;
  cursor: pointer;
  border: none;
  opacity: 0.8;
  transition: opacity 0.2s;
}

input[type='range']::-moz-range-thumb:hover {
  opacity: 1;
}
</style>