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
    config: {
      get: function(key) { return invoke('config_get', { key: key }); },
      set: function(key, value) { return invoke('config_set', { key: key, value: value }); },
      clear: function() { return invoke('config_clear', {}); }
    },
    task: {
      submit: function(type, params) { return invoke('task_submit', { type: type, params: params }); },
      cancel: function(task_id) { return invoke('task_cancel', { task_id: task_id }); },
      pause: function(task_id) { return invoke('task_pause', { task_id: task_id }); }
    },
    log: {
      info: function(msg) { return invoke('log_info', { msg: msg }); },
      error: function(msg) { return invoke('log_error', { msg: msg }); }
    },
    dialog: {
      // 打开原生文件选择对话框,按扩展名过滤。返回路径数组(取消为空数组)。
      openFile: function(extensions, multiple) {
        return invoke('dialog_open_file', { extensions: extensions || [], multiple: !!multiple });
      }
    }
  };
  window.PlugKit = MT;
  window.MT = MT;
})(window);
