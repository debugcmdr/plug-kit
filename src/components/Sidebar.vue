<script setup lang="ts">
import { ref, onMounted } from 'vue'
import type { PluginInfo } from '../types'

const emit = defineEmits<{
  select: [pluginId: string]
}>()

const plugins = ref<PluginInfo[]>([])
const searchQuery = ref('')

onMounted(async () => {
  plugins.value = []
})

const filteredPlugins = () => {
  if (!searchQuery.value) return plugins.value
  const query = searchQuery.value.toLowerCase()
  return plugins.value.filter(p =>
    p.name.toLowerCase().includes(query) ||
    p.description.toLowerCase().includes(query)
  )
}

function handleSelect(pluginId: string) {
  emit('select', pluginId)
}
</script>

<template>
  <n-layout-sider
    :width="240"
    :collapsed-width="60"
    collapse-mode="width"
    :default-collapsed="false"
    show-trigger
    style="height: 100%;"
  >
    <div style="padding: 16px;">
      <n-h3 strong style="margin-bottom: 16px;">PlugKit | 工具市场</n-h3>
      <n-input
        v-model:value="searchQuery"
        placeholder="搜索插件..."
        clearable
        style="margin-bottom: 16px;"
      >
        <template #prefix>
          <n-icon><Search /></n-icon>
        </template>
      </n-input>
    </div>
    
    <n-menu
      :options="filteredPlugins().map(p => ({
        key: p.id,
        label: p.name
      }))"
      @update:value="handleSelect"
      style="padding: 0 8px;"
    />
    
    <div style="padding: 16px;">
      <n-button block type="primary" @click="handleSelect('market')">
        插件市场
      </n-button>
      <n-button block style="margin-top: 8px;" @click="handleSelect('settings')">
        设置
      </n-button>
    </div>
  </n-layout-sider>
</template>
