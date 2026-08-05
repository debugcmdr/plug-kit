import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Task } from '../types'

export const tasks = ref<Task[]>([])

export function addTask(task: Task) {
  tasks.value.push(task)
}

export function updateTask(taskId: string, updates: Partial<Task>) {
  const index = tasks.value.findIndex(t => t.task_id === taskId)
  if (index !== -1) {
    tasks.value[index] = { ...tasks.value[index], ...updates }
  }
}

export function removeTask(taskId: string) {
  tasks.value = tasks.value.filter(t => t.task_id !== taskId)
}

/** Load persisted tasks from the backend (list_tasks command). */
export async function fetchTasks() {
  try {
    tasks.value = await invoke<Task[]>('list_tasks')
  } catch (e) {
    console.error('Failed to load tasks:', e)
  }
}
