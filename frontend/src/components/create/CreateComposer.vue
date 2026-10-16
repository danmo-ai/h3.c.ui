<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { api, type GenerateBody, type ModelStatus, type TaskSnapshot } from '@/utils/api';
import { toast } from '@/utils/feedback';
import MediaPick from './MediaPick.vue';

type Mode = 'prompt' | 'fl2va' | 'ref2va';
type RefKind = 'image' | 'video' | 'audio' | 'video_audio';
type RefRow = {
  kind: RefKind;
  path: string;
  audio_path: string;
  include_embedded_audio: boolean;
};

const ACCEPT_IMAGE = 'image/png,image/jpeg,image/webp,image/bmp,image/gif,.png,.jpg,.jpeg,.webp,.bmp,.gif';
const ACCEPT_VIDEO = 'video/mp4,video/quicktime,video/webm,video/x-matroska,.mp4,.mov,.webm,.mkv,.m4v';
const ACCEPT_AUDIO = 'audio/wav,audio/mpeg,audio/mp4,audio/aac,audio/flac,audio/ogg,.wav,.mp3,.m4a,.aac,.flac,.ogg';

function acceptForKind(kind: RefKind): string {
  if (kind === 'image') return ACCEPT_IMAGE;
  if (kind === 'audio') return ACCEPT_AUDIO;
  if (kind === 'video' || kind === 'video_audio') return ACCEPT_VIDEO;
  return `${ACCEPT_IMAGE},${ACCEPT_VIDEO},${ACCEPT_AUDIO}`;
}

const props = defineProps<{
  status: ModelStatus | null;
}>();

const emit = defineEmits<{
  generated: [];
  failed: [];
  'update:busy': [boolean];
}>();

const { t } = useI18n();
const router = useRouter();

const mode = ref<Mode>('prompt');
const prompt = ref('');
const advancedOpen = ref(false);
const form = reactive({
  width: 864,
  height: 480,
  frames: 56,
  useSeconds: false,
  seconds: 2.3,
  steps: 20,
  seed: 42,
  output_name: '',
  first_frame: '',
  last_frame: '',
  reference_image_size: 'match',
  denoise_reuse: 1,
  dit_layers: 50,
  core_reuse: 1,
  token_reduction: false,
  veda: false,
  turbo_lora: false,
  ssd_streaming: false,
  use_int8_row_fc2: false,
  use_reference_rope: false,
  use_slower_bf16_mlp: false,
  use_slower_bf16_qkv: false,
  use_slower_bf16_attention_output: false,
  use_slower_row_major_attention_output: false,
  use_slower_unfused_int8_inputs: false,
  use_slower_unfused_qkv_rope: false,
  use_slower_scalar_qkv_rms: false,
  use_slower_uncached_int8_scales: false,
  use_slower_dynamic_fc1_k: false,
  use_slower_grouped_quantizer: false,
  render_width: 0,
  render_height: 0,
  preview_denoise: true,
});
const references = ref<RefRow[]>([]);
const task = ref<TaskSnapshot | null>(null);
let es: EventSource | null = null;

type AttentionAccel = 'off' | 'token_reduction' | 'veda';
type MemoryMode = 'off' | 'turbo_lora' | 'ssd_streaming';

/** Mutually exclusive attention accelerators — engine rejects veda+token_reduction. */
const attentionAccel = computed<AttentionAccel>({
  get() {
    if (form.veda) return 'veda';
    if (form.token_reduction) return 'token_reduction';
    return 'off';
  },
  set(value) {
    form.veda = value === 'veda';
    form.token_reduction = value === 'token_reduction';
  },
});

/** Mutually exclusive residency / schedule modes — engine rejects turbo+ssd. */
const memoryMode = computed<MemoryMode>({
  get() {
    if (form.turbo_lora) return 'turbo_lora';
    if (form.ssd_streaming) return 'ssd_streaming';
    return 'off';
  },
  set(value) {
    form.turbo_lora = value === 'turbo_lora';
    form.ssd_streaming = value === 'ssd_streaming';
  },
});

const attentionAccelOptions = computed(() =>
  [
    { value: 'off' as const, label: t('create.accelOff') },
    { value: 'token_reduction' as const, label: t('create.sw.tokenReduction') },
    { value: 'veda' as const, label: t('create.sw.veda') },
  ] as const,
);

const memoryModeOptions = computed(() =>
  [
    { value: 'off' as const, label: t('create.accelOff') },
    { value: 'turbo_lora' as const, label: t('create.sw.turboLora') },
    { value: 'ssd_streaming' as const, label: t('create.sw.ssdStreaming') },
  ] as const,
);

const speedSwitches = computed(() =>
  [
    { key: 'use_int8_row_fc2' as const, label: t('create.sw.int8RowFc2') },
    { key: 'use_reference_rope' as const, label: t('create.sw.referenceRope') },
  ] as const,
);

const compatSwitches = computed(() =>
  [
    { key: 'use_slower_bf16_mlp' as const, label: t('create.sw.slowerBf16Mlp') },
    { key: 'use_slower_bf16_qkv' as const, label: t('create.sw.slowerBf16Qkv') },
    { key: 'use_slower_bf16_attention_output' as const, label: t('create.sw.slowerBf16AttnOut') },
    {
      key: 'use_slower_row_major_attention_output' as const,
      label: t('create.sw.slowerRowMajorAttn'),
    },
    { key: 'use_slower_unfused_int8_inputs' as const, label: t('create.sw.slowerUnfusedInt8') },
    { key: 'use_slower_unfused_qkv_rope' as const, label: t('create.sw.slowerUnfusedQkvRope') },
    { key: 'use_slower_scalar_qkv_rms' as const, label: t('create.sw.slowerScalarQkvRms') },
    {
      key: 'use_slower_uncached_int8_scales' as const,
      label: t('create.sw.slowerUncachedInt8'),
    },
    { key: 'use_slower_dynamic_fc1_k' as const, label: t('create.sw.slowerDynamicFc1') },
    {
      key: 'use_slower_grouped_quantizer' as const,
      label: t('create.sw.slowerGroupedQuant'),
    },
  ] as const,
);

const busy = computed(
  () => !!task.value && (task.value.status === 'queued' || task.value.status === 'running'),
);

watch(busy, (v) => emit('update:busy', v), { immediate: true });

watch(memoryMode, (mode) => {
  if (mode === 'turbo_lora' && form.steps === 20) form.steps = 8;
});

const canGenerate = computed(() => {
  if (!props.status?.fl2va_ready) return false;
  if (!prompt.value.trim()) return false;
  if (busy.value) return false;
  if (mode.value === 'fl2va' && !form.first_frame.trim()) return false;
  if (mode.value === 'ref2va') {
    if (!props.status.ref2va_ready) return false;
    if (!references.value.some((r) => r.path.trim())) return false;
    if (
      references.value.some(
        (r) => r.kind === 'video_audio' && r.path.trim() && !r.audio_path.trim(),
      )
    ) {
      return false;
    }
  }
  return true;
});

const previewSrc = computed(() =>
  task.value?.preview_jpeg_base64
    ? `data:image/jpeg;base64,${task.value.preview_jpeg_base64}`
    : '',
);

function closeEs() {
  es?.close();
  es = null;
}

function addRef() {
  references.value.push({
    kind: 'image',
    path: '',
    audio_path: '',
    include_embedded_audio: true,
  });
}

function removeRef(i: number) {
  references.value.splice(i, 1);
}

function onRefKindChange(r: RefRow) {
  r.path = '';
  r.audio_path = '';
}

function buildBody(): GenerateBody {
  const body: GenerateBody = {
    prompt: prompt.value.trim(),
    width: form.width,
    height: form.height,
    steps: form.steps,
    seed: form.seed,
    output_name: form.output_name || null,
    first_frame: null,
    last_frame: null,
    references: [],
    reference_image_size: form.reference_image_size,
    denoise_reuse: form.denoise_reuse,
    dit_layers: form.dit_layers,
    core_reuse: form.core_reuse,
    token_reduction: form.token_reduction,
    veda: form.veda,
    turbo_lora: form.turbo_lora,
    ssd_streaming: form.ssd_streaming,
    use_int8_row_fc2: form.use_int8_row_fc2,
    use_reference_rope: form.use_reference_rope,
    use_slower_bf16_mlp: form.use_slower_bf16_mlp,
    use_slower_bf16_qkv: form.use_slower_bf16_qkv,
    use_slower_bf16_attention_output: form.use_slower_bf16_attention_output,
    use_slower_row_major_attention_output: form.use_slower_row_major_attention_output,
    use_slower_unfused_int8_inputs: form.use_slower_unfused_int8_inputs,
    use_slower_unfused_qkv_rope: form.use_slower_unfused_qkv_rope,
    use_slower_scalar_qkv_rms: form.use_slower_scalar_qkv_rms,
    use_slower_uncached_int8_scales: form.use_slower_uncached_int8_scales,
    use_slower_dynamic_fc1_k: form.use_slower_dynamic_fc1_k,
    use_slower_grouped_quantizer: form.use_slower_grouped_quantizer,
    render_width: form.render_width,
    render_height: form.render_height,
    preview_denoise: form.preview_denoise,
  };
  if (form.useSeconds) {
    body.seconds = form.seconds;
    body.frames = null;
  } else {
    body.frames = form.frames;
    body.seconds = null;
  }
  if (mode.value === 'fl2va') {
    body.first_frame = form.first_frame || null;
    body.last_frame = form.last_frame || null;
  }
  if (mode.value === 'ref2va') {
    body.references = references.value
      .filter((r) => r.path.trim())
      .map((r) => ({
        kind: r.kind,
        path: r.path.trim(),
        audio_path: r.audio_path.trim() || null,
        include_embedded_audio: r.include_embedded_audio,
      }));
  }
  return body;
}

async function generate() {
  try {
    if (form.turbo_lora && props.status && props.status.turbo_lora_ready === false) {
      toast.error(t('create.turboLoraMissing'));
      return;
    }
    const { task_id } = await api.generate(buildBody());
    closeEs();
    es = new EventSource(api.taskStreamUrl(task_id));
    es.addEventListener('progress', (ev) => {
      const data = JSON.parse((ev as MessageEvent).data) as TaskSnapshot;
      task.value = data;
      if (data.status === 'completed') {
        toast.success('Generation complete');
        emit('generated');
        closeEs();
      } else if (data.status === 'failed') {
        toast.error(data.error || 'Generation failed');
        emit('failed');
        closeEs();
      } else if (data.status === 'cancelled') {
        toast.notify({ title: 'Cancelled', message: '', type: 'warning' });
        closeEs();
      }
    });
  } catch (e) {
    toast.error(String((e as Error).message || e));
  }
}

async function cancel() {
  if (!task.value) return;
  try {
    await api.cancelTask(task.value.task_id);
  } catch (e) {
    toast.error(String((e as Error).message || e));
  }
}

defineExpose({ closeEs });
</script>

<template>
  <div class="h3-composer">
    <div class="h3-composer__scroll">
      <DqAlert v-if="status && !status.fl2va_ready" type="warning" class="h3-composer__alert">
        <div class="h3-composer__alert-row">
          <span>{{ t('create.notReady') }}</span>
          <DqButton size="sm" type="primary" @click="router.push({ name: 'download' })">
            {{ t('create.goDownload') }}
          </DqButton>
        </div>
      </DqAlert>

      <DqAlert
        v-if="mode === 'ref2va' && status && !status.ref2va_ready"
        type="warning"
        class="h3-composer__alert"
      >
        {{ t('create.ref2vaMissing') }}
      </DqAlert>

      <!-- Mode -->
      <section class="h3-composer__section">
        <DqSegmented
          size="sm"
          :model-value="mode"
          :options="[
            { value: 'prompt', label: t('create.modePrompt') },
            { value: 'fl2va', label: t('create.modeFl2va') },
            { value: 'ref2va', label: t('create.modeRef2va') },
          ]"
          @update:model-value="(v: string) => (mode = v as Mode)"
        />
      </section>

      <!-- Prompt hero -->
      <section class="h3-composer__section h3-composer__prompt">
        <DqInput
          v-model="prompt"
          type="textarea"
          :rows="5"
          resize="none"
          :placeholder="t('create.promptPlaceholder')"
        />
      </section>

      <!-- FL2VA frames -->
      <section v-if="mode === 'fl2va'" class="h3-composer__section">
        <div class="h3-composer__section-label">{{ t('create.sectionFrames') }}</div>
        <div class="h3-composer__grid">
          <div class="h3-composer__cell">
            <span>{{ t('create.firstFrame') }}</span>
            <MediaPick v-model="form.first_frame" :accept="ACCEPT_IMAGE" :label="t('create.pickFirst')" />
          </div>
          <div class="h3-composer__cell">
            <span>{{ t('create.lastFrame') }}</span>
            <MediaPick v-model="form.last_frame" :accept="ACCEPT_IMAGE" :label="t('create.pickLast')" />
          </div>
        </div>
      </section>

      <!-- Ref2VA -->
      <section v-if="mode === 'ref2va'" class="h3-composer__section">
        <div class="h3-composer__section-head">
          <div class="h3-composer__section-label">{{ t('create.references') }}</div>
          <DqButton size="sm" variant="ghost" @click="addRef">{{ t('create.addRef') }}</DqButton>
        </div>
        <label class="h3-composer__cell">
          <span>{{ t('create.refSize') }}</span>
          <DqSelect v-model="form.reference_image_size" size="sm">
            <DqOption value="match" :label="t('create.refSizeMatch')" />
            <DqOption value="max" :label="t('create.refSizeMax')" />
          </DqSelect>
        </label>
        <div v-if="!references.length" class="h3-composer__empty">{{ t('create.refsEmpty') }}</div>
        <div v-for="(r, i) in references" :key="i" class="h3-composer__ref">
          <DqSelect v-model="r.kind" size="sm" @update:model-value="onRefKindChange(r)">
            <DqOption value="image" :label="t('create.refKindImage')" />
            <DqOption value="video" :label="t('create.refKindVideo')" />
            <DqOption value="audio" :label="t('create.refKindAudio')" />
            <DqOption value="video_audio" :label="t('create.refKindVideoAudio')" />
          </DqSelect>
          <MediaPick
            :key="`${i}-${r.kind}-main`"
            v-model="r.path"
            :variant="r.kind === 'image' ? 'image' : 'file'"
            :accept="acceptForKind(r.kind)"
            :label="r.kind === 'image' ? t('create.pickImage') : t('create.pickFile')"
          />
          <MediaPick
            v-if="r.kind === 'video_audio'"
            :key="`${i}-audio`"
            v-model="r.audio_path"
            variant="file"
            :accept="ACCEPT_AUDIO"
            :label="t('create.pickAudio')"
          />
          <DqPrefRow v-if="r.kind === 'video'" :label="t('create.sw.embedAudio')">
            <DqSwitch v-model="r.include_embedded_audio" size="sm" />
          </DqPrefRow>
          <DqButton size="sm" variant="ghost" @click="removeRef(i)">{{ t('create.remove') }}</DqButton>
        </div>
      </section>

      <!-- Core params -->
      <section class="h3-composer__section">
        <div class="h3-composer__section-label">{{ t('create.basic') }}</div>
        <div class="h3-composer__grid">
          <label class="h3-composer__cell">
            <span>{{ t('create.width') }}</span>
            <DqInputNumber v-model="form.width" size="sm" :min="64" :step="32" />
          </label>
          <label class="h3-composer__cell">
            <span>{{ t('create.height') }}</span>
            <DqInputNumber v-model="form.height" size="sm" :min="64" :step="32" />
          </label>
          <label class="h3-composer__cell">
            <span>{{ t('create.steps') }}</span>
            <DqInputNumber v-model="form.steps" size="sm" :min="1" />
          </label>
          <label class="h3-composer__cell">
            <span>{{ t('create.seed') }}</span>
            <DqInputNumber v-model="form.seed" size="sm" :min="0" />
          </label>
          <label class="h3-composer__cell">
            <span>{{ t('create.durationUnit') }}</span>
            <DqSegmented
              block
              size="sm"
              :model-value="form.useSeconds ? 'seconds' : 'frames'"
              :options="[
                { value: 'frames', label: t('create.frames') },
                { value: 'seconds', label: t('create.seconds') },
              ]"
              @update:model-value="(v: string) => (form.useSeconds = v === 'seconds')"
            />
          </label>
          <label class="h3-composer__cell">
            <span>{{ form.useSeconds ? t('create.seconds') : t('create.frames') }}</span>
            <DqInputNumber
              v-if="!form.useSeconds"
              v-model="form.frames"
              size="sm"
              :min="1"
            />
            <DqInputNumber
              v-else
              v-model="form.seconds"
              size="sm"
              :min="0.1"
              :step="0.1"
            />
          </label>
          <label class="h3-composer__cell h3-composer__cell--full">
            <span>{{ t('create.outputName') }}</span>
            <DqInput v-model="form.output_name" size="sm" :placeholder="t('create.outputAuto')" />
          </label>
        </div>
      </section>

      <!-- Advanced -->
      <section class="h3-composer__section h3-composer__advanced">
        <button
          type="button"
          class="h3-composer__advanced-toggle"
          :aria-expanded="advancedOpen"
          @click="advancedOpen = !advancedOpen"
        >
          <span>{{ t('create.advanced') }}</span>
          <span class="h3-composer__chevron" :class="{ 'is-open': advancedOpen }">▾</span>
        </button>

        <div v-if="advancedOpen" class="h3-composer__advanced-body">
          <div class="h3-composer__grid">
            <label class="h3-composer__cell">
              <span>{{ t('create.denoiseReuse') }}</span>
              <DqInputNumber v-model="form.denoise_reuse" size="sm" :min="1" :max="3" />
            </label>
            <label class="h3-composer__cell">
              <span>{{ t('create.ditLayers') }}</span>
              <DqInputNumber v-model="form.dit_layers" size="sm" :min="35" :max="50" />
            </label>
            <label class="h3-composer__cell">
              <span>{{ t('create.coreReuse') }}</span>
              <DqInputNumber v-model="form.core_reuse" size="sm" :min="1" />
            </label>
            <label class="h3-composer__cell">
              <span>{{ t('create.renderWidth') }}</span>
              <DqInputNumber v-model="form.render_width" size="sm" :min="0" />
            </label>
            <label class="h3-composer__cell">
              <span>{{ t('create.renderHeight') }}</span>
              <DqInputNumber v-model="form.render_height" size="sm" :min="0" />
            </label>
          </div>

          <div class="h3-composer__subsection">{{ t('create.sectionSpeed') }}</div>
          <p class="h3-composer__hint">{{ t('create.mutexHint') }}</p>
          <div class="h3-composer__prefs">
            <DqPrefRow :label="t('create.attentionAccel')">
              <DqSelect v-model="attentionAccel" size="sm" class="h3-composer__mutex-select">
                <DqOption
                  v-for="opt in attentionAccelOptions"
                  :key="opt.value"
                  :value="opt.value"
                  :label="opt.label"
                />
              </DqSelect>
            </DqPrefRow>
            <DqPrefRow :label="t('create.memoryMode')">
              <DqSelect v-model="memoryMode" size="sm" class="h3-composer__mutex-select">
                <DqOption
                  v-for="opt in memoryModeOptions"
                  :key="opt.value"
                  :value="opt.value"
                  :label="opt.label"
                />
              </DqSelect>
            </DqPrefRow>
            <DqPrefRow v-for="item in speedSwitches" :key="item.key" :label="item.label">
              <DqSwitch v-model="form[item.key]" size="sm" />
            </DqPrefRow>
          </div>

          <div class="h3-composer__subsection">{{ t('create.sectionCompat') }}</div>
          <div class="h3-composer__prefs">
            <DqPrefRow v-for="item in compatSwitches" :key="item.key" :label="item.label">
              <DqSwitch v-model="form[item.key]" size="sm" />
            </DqPrefRow>
          </div>
        </div>
      </section>

      <!-- Live progress -->
      <section v-if="task" class="h3-composer__section h3-composer__live">
        <div class="h3-composer__live-meta">
          <span>{{ task.phase }}</span>
          <span v-if="task.status === 'failed'" class="h3-composer__live-failed">
            {{ t('create.failedLabel') }}
          </span>
          <span v-else-if="task.status === 'cancelled'">{{ t('create.cancelledLabel') }}</span>
          <span v-else-if="task.status === 'completed'">100%</span>
          <span v-else>{{ Math.round(task.progress) }}%</span>
        </div>
        <DqProgress
          :percentage="
            task.status === 'failed'
              ? Math.min(Math.round(task.progress), 99)
              : Math.round(task.progress)
          "
        />
        <p v-if="task.status === 'failed' && task.error" class="h3-composer__live-error">
          {{ task.error }}
        </p>
        <img v-if="previewSrc" class="h3-preview" :src="previewSrc" alt="preview" />
      </section>
    </div>

    <!-- Sticky footer -->
    <footer class="h3-composer__footer">
      <label class="h3-composer__footer-preview">
        <span>{{ t('create.previewShort') }}</span>
        <DqSwitch v-model="form.preview_denoise" size="sm" />
      </label>
      <div class="h3-composer__footer-actions">
        <DqButton size="sm" variant="ghost" :disabled="!busy" @click="cancel">
          {{ t('create.cancel') }}
        </DqButton>
        <DqButton size="sm" type="primary" :disabled="!canGenerate" @click="generate">
          {{ t('create.generate') }}
        </DqButton>
      </div>
    </footer>
  </div>
</template>

<style scoped>
.h3-composer {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.h3-composer__scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding-bottom: 8px;
}

.h3-composer__alert-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
}

.h3-composer__section {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.h3-composer__section-label {
  font-size: var(--dq-font-size-caption, 12px);
  font-weight: 600;
  letter-spacing: 0.02em;
  text-transform: uppercase;
  opacity: 0.55;
}

.h3-composer__section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.h3-composer__prompt :deep(textarea) {
  min-height: 120px;
  font-size: var(--dq-font-size-body, 14px);
  line-height: 1.45;
}

.h3-composer__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px 12px;
  align-items: start;
}

.h3-composer__cell {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
  max-width: 100%;
}

.h3-composer__cell--full {
  grid-column: 1 / -1;
}

.h3-composer__cell > span {
  font-size: var(--dq-font-size-caption, 12px);
  opacity: 0.62;
}

/* Same column width for InputNumber / Segmented / Input. */
.h3-composer__cell :deep(.dq-input-number),
.h3-composer__cell :deep(.dq-input),
.h3-composer__cell :deep(.dq-select),
.h3-composer__cell :deep(.dq-segmented) {
  width: 100%;
  max-width: 100%;
  min-width: 0;
  box-sizing: border-box;
}

.h3-composer__prefs {
  display: flex;
  flex-direction: column;
}

.h3-composer__prefs :deep(.dq-pref-row) {
  min-height: 34px;
  padding: 7px 0;
  border-top: 0.5px solid color-mix(in srgb, var(--dq-label-primary, #fff) 7%, transparent);
}

.h3-composer__prefs :deep(.dq-pref-row__label) {
  font-size: var(--dq-font-size-body, 14px);
  opacity: 0.88;
}

.h3-composer__prefs :deep(.dq-pref-row__content) {
  display: flex;
  justify-content: flex-end;
}

.h3-composer__ref {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 0;
  border-top: 0.5px solid color-mix(in srgb, var(--dq-label-primary, #fff) 8%, transparent);
}

.h3-composer__empty {
  font-size: var(--dq-font-size-caption, 12px);
  opacity: 0.5;
}

.h3-composer__advanced {
  padding-top: 4px;
  border-top: 0.5px solid color-mix(in srgb, var(--dq-label-primary, #fff) 8%, transparent);
}

.h3-composer__advanced-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: none;
  background: none;
  padding: 4px 0;
  font: inherit;
  font-size: var(--dq-font-size-caption, 12px);
  font-weight: 600;
  color: var(--dq-label-secondary, inherit);
  cursor: pointer;
  opacity: 0.75;
}

.h3-composer__advanced-toggle:hover {
  opacity: 1;
}

.h3-composer__chevron {
  display: inline-block;
  transition: transform 0.15s ease;
}

.h3-composer__chevron.is-open {
  transform: rotate(180deg);
}

.h3-composer__advanced-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-top: 4px;
}

.h3-composer__subsection {
  font-size: var(--dq-font-size-caption, 12px);
  font-weight: 600;
  opacity: 0.5;
  margin-top: 4px;
}

.h3-composer__hint {
  margin: -2px 0 0;
  font-size: var(--dq-font-size-caption, 12px);
  line-height: 1.4;
  opacity: 0.55;
}

.h3-composer__mutex-select {
  min-width: 168px;
  max-width: 220px;
}

.h3-composer__live {
  gap: 8px;
}

.h3-composer__live-meta {
  display: flex;
  justify-content: space-between;
  font-size: var(--dq-font-size-caption, 12px);
  opacity: 0.65;
}

.h3-composer__live-failed {
  color: var(--dq-danger, #ff453a);
  opacity: 1;
  font-weight: 650;
}

.h3-composer__live-error {
  margin: 0;
  font-size: var(--dq-font-size-caption, 12px);
  line-height: 1.45;
  color: var(--dq-danger, #ff453a);
  word-break: break-word;
  white-space: pre-wrap;
}

.h3-composer__footer {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-top: 12px;
  margin-top: 4px;
  border-top: 0.5px solid color-mix(in srgb, var(--dq-label-primary, #fff) 10%, transparent);
  background: transparent;
}

.h3-composer__footer-preview {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: var(--dq-font-size-caption, 12px);
  opacity: 0.78;
  cursor: pointer;
  user-select: none;
}

.h3-composer__footer-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
</style>
