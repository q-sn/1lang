<script setup lang="ts">
import { onActivated, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { commands, unwrap, type UsageRow, type UsageStats } from '@/lib/api';
import { formatCost } from '@/lib/translator';
import Button from '@/ui/Button.vue';
import Card from '@/ui/Card.vue';

const { t } = useI18n();
const stats = ref<UsageStats | null>(null);
const confirmReset = ref(false);

async function load() {
  stats.value = await unwrap(commands.usageStats());
}
onMounted(load);
onActivated(load);

const periods = ['today', 'month', 'total'] as const;
const total = (rows: UsageRow[]) => rows.reduce((s, r) => s + (r.cost_usd ?? 0), 0);
const num = (n: number | null) => (n ?? 0).toLocaleString();

async function reset() {
  if (!confirmReset.value) {
    confirmReset.value = true;
    setTimeout(() => (confirmReset.value = false), 3000);
    return;
  }
  await commands.usageReset();
  confirmReset.value = false;
  load();
}
</script>

<template>
  <div v-if="stats">
    <Card v-for="p in periods" :key="p" :title="`${t(`settings.usage.${p}`)} · ${formatCost(total(stats[p])) || '$0'}`">
      <div v-if="!stats[p].length" class="px-4 py-3 text-[12.5px] text-faint">{{ t('settings.usage.empty') }}</div>
      <table v-else class="w-full text-[12.5px]">
        <thead class="text-left text-[11.5px] text-faint">
          <tr class="border-b border-line">
            <th class="px-4 py-2 font-medium">{{ t('settings.usage.provider') }}</th>
            <th class="px-2 py-2 text-right font-medium">{{ t('settings.usage.requests') }}</th>
            <th class="px-2 py-2 text-right font-medium">{{ t('settings.usage.tokens') }}</th>
            <th class="px-2 py-2 text-right font-medium">{{ t('settings.usage.characters') }}</th>
            <th class="px-4 py-2 text-right font-medium">{{ t('settings.usage.cost') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in stats[p]" :key="r.provider_id" class="border-b border-line last:border-b-0">
            <td class="px-4 py-2">{{ r.provider_name }}</td>
            <td class="px-2 py-2 text-right tabular-nums">{{ num(r.requests) }}</td>
            <td class="px-2 py-2 text-right tabular-nums">{{ num(r.input_tokens) }} / {{ num(r.output_tokens) }}</td>
            <td class="px-2 py-2 text-right tabular-nums">{{ num(r.characters) }}</td>
            <td class="px-4 py-2 text-right tabular-nums">{{ formatCost(r.cost_usd) || '—' }}</td>
          </tr>
        </tbody>
      </table>
    </Card>
    <div class="flex items-center gap-3 px-1">
      <p class="flex-1 text-[12px] text-muted">{{ t('settings.usage.note') }}</p>
      <Button variant="danger" size="sm" @click="reset">
        {{ confirmReset ? '?' : '' }} {{ t('settings.usage.reset') }}
      </Button>
    </div>
  </div>
</template>
