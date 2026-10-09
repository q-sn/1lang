<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from 'reka-ui';
import { Plus } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { commands, type KeyStatus, type ProviderKind, type ProviderKindInfo } from '@/lib/api';
import { useSettings } from '@/lib/settings';
import Button from '@/ui/Button.vue';
import ProviderCard from './ProviderCard.vue';

const { t } = useI18n();
const { settings } = useSettings();
const kinds = ref<ProviderKindInfo[]>([]);
const keys = ref<KeyStatus[]>([]);

async function refreshKeys() {
  keys.value = await commands.keyStatuses();
}

onMounted(async () => {
  kinds.value = await commands.providerKinds();
  await refreshKeys();
});

const llmKinds = computed(() => kinds.value.filter((k) => k.is_llm));
const mtKinds = computed(() => kinds.value.filter((k) => !k.is_llm));
const kindInfo = (k: ProviderKind) => kinds.value.find((i) => i.kind === k);

function add(info: ProviderKindInfo) {
  const id = `${info.kind}-${Math.random().toString(36).slice(2, 7)}`;
  settings.value.providers.push({
    id,
    kind: info.kind,
    name: info.label,
    enabled: true,
    base_url: null,
    model: info.default_model,
    region: null,
    temperature: null,
    extra_body: null,
    price_input: null,
    price_output: null,
  });
}

function move(i: number, dir: -1 | 1) {
  const list = settings.value.providers;
  const j = i + dir;
  if (j < 0 || j >= list.length) return;
  [list[i], list[j]] = [list[j], list[i]];
}

async function remove(i: number) {
  const p = settings.value.providers[i];
  await commands.setApiKey(p.id, null);
  settings.value.providers.splice(i, 1);
  refreshKeys();
}
</script>

<template>
  <div>
    <p class="mb-4 px-1 text-[14px] leading-snug text-muted">{{ t('settings.providers.desc') }}</p>

    <div class="space-y-3">
      <ProviderCard
        v-for="(p, i) in settings.providers"
        :key="p.id"
        :provider="p"
        :info="kindInfo(p.kind)"
        :key-hint="keys.find((k) => k.id === p.id)?.hint ?? null"
        :index="i"
        :count="settings.providers.length"
        @move="(d) => move(i, d)"
        @remove="remove(i)"
        @key-changed="refreshKeys"
      />
    </div>

    <DropdownMenuRoot>
      <DropdownMenuTrigger as-child>
        <Button class="mt-3"><Plus class="size-4" />{{ t('settings.providers.add') }}</Button>
      </DropdownMenuTrigger>
      <DropdownMenuPortal>
        <DropdownMenuContent
          :side-offset="6"
          :collision-padding="12"
          align="start"
          class="menu z-50 max-h-[var(--reka-dropdown-menu-content-available-height)] min-w-64 overflow-y-auto p-1.5 animate-pop-in"
        >
          <DropdownMenuLabel class="px-3 pb-1.5 pt-2 text-[11.5px] font-semibold uppercase tracking-wider text-faint">
            {{ t('settings.providers.llm') }}
          </DropdownMenuLabel>
          <DropdownMenuItem
            v-for="k in llmKinds"
            :key="k.kind"
            class="flex h-10 cursor-default items-center rounded-[10px] px-3 text-[14px] outline-none data-[highlighted]:bg-hover"
            @select="add(k)"
          >
            {{ k.label }}
          </DropdownMenuItem>
          <DropdownMenuSeparator class="my-1 h-px bg-line" />
          <DropdownMenuLabel class="px-3 pb-1.5 pt-2 text-[11.5px] font-semibold uppercase tracking-wider text-faint">
            {{ t('settings.providers.mt') }}
          </DropdownMenuLabel>
          <DropdownMenuItem
            v-for="k in mtKinds"
            :key="k.kind"
            class="flex h-10 cursor-default items-center rounded-[10px] px-3 text-[14px] outline-none data-[highlighted]:bg-hover"
            @select="add(k)"
          >
            {{ k.label }}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenuPortal>
    </DropdownMenuRoot>
  </div>
</template>
