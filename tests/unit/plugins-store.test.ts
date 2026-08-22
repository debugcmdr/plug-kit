import { describe, it, expect } from 'vitest'
import { mapTask } from '../../src/stores/plugins'

describe('mapTask（后端 list_tasks 原始 JSON -> 前端 Task）', () => {
  it('映射 type 字段（回归 P0-2：后端 serde rename 输出 "type"）', () => {
    const raw = {
      task_id: 't-1',
      plugin_id: 'download',
      type: 'download',
      status: 'running',
      progress: { percent: 42, speed: '2.3MB/s', eta: '10s', message: '下载中 42%' },
      created_at: '2026-08-21T00:00:00Z',
      started_at: '2026-08-21T00:00:01Z',
      completed_at: null,
    }
    const task = mapTask(raw as Record<string, unknown>)
    expect(task.type).toBe('download')
    expect(task.status).toBe('running')
    expect(task.progress.percent).toBe(42)
    expect(task.progress.speed).toBe('2.3MB/s')
    expect(task.progress.message).toBe('下载中 42%')
    expect(task.started_at).toBe('2026-08-21T00:00:01Z')
  })

  it('缺字段时安全回退（type 为空、status=pending、progress=0）', () => {
    const task = mapTask({ task_id: 'x' } as Record<string, unknown>)
    expect(task.type).toBe('')
    expect(task.status).toBe('pending')
    expect(task.progress.percent).toBe(0)
    expect(task.completed_at).toBeUndefined()
  })

  it('映射失败原因 error 字段', () => {
    const task = mapTask({ task_id: 't-2', error: '输入文件不存在: /x.mp4' } as Record<string, unknown>)
    expect(task.error).toBe('输入文件不存在: /x.mp4')
    // 无 error 时回退 undefined
    expect(mapTask({ task_id: 't-3' } as Record<string, unknown>).error).toBeUndefined()
  })
})
