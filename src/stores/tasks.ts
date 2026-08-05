import { ref } from 'vue'
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
