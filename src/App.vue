<script setup lang="ts">
import { ref } from 'vue'
import Sidebar from './components/Sidebar.vue'
import MainContent from './views/MainContent.vue'
import TaskBar from './components/TaskBar.vue'

const activePlugin = ref<string | null>(null)

function selectPlugin(pluginId: string) {
  activePlugin.value = pluginId
}
</script>

<template>
  <n-config-provider>
    <n-message-provider>
      <n-layout has-sider style="height: 100vh;">
        <Sidebar @select="selectPlugin" />
        <n-layout>
          <n-layout-content style="padding: 16px;">
            <MainContent :active-plugin="activePlugin" />
          </n-layout-content>
          <TaskBar />
        </n-layout>
      </n-layout>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body, #app {
  height: 100%;
  width: 100%;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}
</style>
