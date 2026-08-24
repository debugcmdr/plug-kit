(function(window) {
  'use strict';
  const CHANNEL = 'plugkit';
  const BRIDGE_VERSION = '1.0.0';
  const pending = new Map();
  
  function invoke(command, payload) {
    return new Promise((resolve, reject) => {
      const id = typeof crypto !== 'undefined' && crypto.randomUUID 
        ? crypto.randomUUID() 
        : 'id-' + Date.now();
      pending.set(id, { resolve, reject, timer: null });
      window.parent.postMessage({
        type: CHANNEL + ':invoke',
        id: id,
        command: command,
        payload: payload || {}
      }, '*');
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error('Command ' + command + ' timed out'));
      }, 600000);  // 10 分钟(ASR/dialog 等长操作)
      pending.get(id).timer = timer;
    });
  }
  
  function onProgress(callback) {
    const handler = (e) => {
      if (e.data && e.data.type === CHANNEL + ':progress') {
        callback(e.data);
      }
    };
    window.addEventListener('message', handler);
    return function() { window.removeEventListener('message', handler); };
  }

  // 监听外壳转发的系统文件拖放事件(plugkit:files-dropped → 激活插件 iframe)。
  // 回调收到路径数组,由插件侧按自身业务过滤(如扩展名)。
  function onFilesDropped(callback) {
    const handler = (e) => {
      if (e.data && e.data.type === CHANNEL + ':files-dropped') {
        callback((e.data && e.data.paths) || []);
      }
    };
    window.addEventListener('message', handler);
    return function() { window.removeEventListener('message', handler); };
  }

  // 接收主程序回发的 plugkit:response,按 id resolve/reject 对应的 invoke Promise。
  // (修复:原先没有任何地方消费 response,所有 invoke 永远 pending 直到超时)
  window.addEventListener('message', (e) => {
    const data = e.data;
    if (!data || data.type !== CHANNEL + ':response') return;
    const p = pending.get(data.id);
    if (!p) return;
    pending.delete(data.id);
    if (p.timer) clearTimeout(p.timer);
    const r = data.result;
    if (r && typeof r === 'object' && 'Err' in r) {
      p.reject(new Error(String(r.Err)));
    } else if (r && typeof r === 'object' && 'Ok' in r) {
      p.resolve(r.Ok);   // 解包 Rust Result 的 Ok 值
    } else {
      p.resolve(r);
    }
  });
  
  const MT = {
    invoke: invoke,
    onProgress: onProgress,
    onFilesDropped: onFilesDropped,
    config: {
      get: function(key) { return invoke('config_get', { key: key }); },
      set: function(key, value) { return invoke('config_set', { key: key, value: value }); }
      // 注意:SDK 能力与后端 dispatch 一一对应。后端无 config_clear,
      // 未提供 clear()(调用会落入兜底分支被误当作插件命令)。
    },
    // 文件系统只读辅助:stat(path) → { size }(展示已选文件体积)。
    fs: {
      stat: function(path) { return invoke('stat_file', { path: path }); }
    },
    // 任务控制:取消 / 暂停 / 恢复。
    // 任务本身由 MT.invoke 自动创建(带 URL 去重:活跃任务不重复创建,终态允许重新解析)。
    task: {
      cancel: function(task_id) { return invoke('task_cancel', { task_id: task_id }); },
      pause: function(task_id) { return invoke('task_pause', { task_id: task_id }); },
      resume: function(task_id) { return invoke('task_resume', { task_id: task_id }); }
    },
    log: {
      info: function(msg) { return invoke('log_info', { msg: msg }); },
      error: function(msg) { return invoke('log_error', { msg: msg }); }
    },
    dialog: {
      // 打开原生文件选择对话框,按扩展名过滤。返回路径数组(取消为空数组)。
      openFile: function(extensions, multiple) {
        return invoke('dialog_open_file', { extensions: extensions || [], multiple: !!multiple });
      },
      // 选择文件夹。返回 { path } 或 { path: null }。
      openFolder: function() {
        return invoke('dialog_open_folder', {});
      }
    },
    // 在系统文件管理器中打开路径(文件夹或文件所在目录)
    openPath: function(path) {
      return invoke('open_in_folder', { path: path });
    },
    // 系统级能力
    system: {
      // 打开 macOS「完全磁盘访问权限」设置面板(Safari/浏览器 cookies 授权)。
      // 与后端命令一一对应;非 macOS 平台会返回错误说明。
      openPermissionSettings: function() {
        return invoke('open_permission_settings', {});
      }
    }
  };
  window.PlugKit = MT;
  window.MT = MT;
})(window);
