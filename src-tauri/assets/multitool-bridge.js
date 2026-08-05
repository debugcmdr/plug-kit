(function(window) {
  'use strict';
  const CHANNEL = 'multitool';
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
      }, 60000);
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
    }
  };
  window.Multitool = MT;
  window.MT = MT;
})(window);
