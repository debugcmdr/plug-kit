<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  pluginId: string
}>()

// 桥接版本兼容性由两道安装期关卡校验:市场安装(plugin_manager.install 校验
// bridgeVersion)与预装(preinstall 复制前校验),运行时不再重复检查——
// 手动改插件文件属用户自担场景,不值得保留一条运行时补救路径。
const iframeSrc = computed(() =>
  `plugkit://plugin/${props.pluginId}/tool/index.html`
)
</script>

<template>
  <div style="display:flex; flex-direction:column; height:100%;">
    <!-- 内容区：iframe 自身控制内边距，容器只负责背景/圆角/溢出 -->
    <div style="flex:1; min-height:0; background:var(--n-color-target); border-radius:0 0 8px 8px; overflow:hidden;">
      <iframe
        :src="iframeSrc"
        :data-plugin-id="props.pluginId"
        data-plugin
        style="width:100%; height:100%; border:none; display:block;"
        sandbox="allow-scripts allow-forms"
      />
    </div>
  </div>
</template>
