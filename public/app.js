// ===== State =====
let nodes = new Map(); // id -> node
let rootNodes = [];    // root node ids
let currentPath = '';
let isScanning = false;
let isConverting = false;
let lastClickedFileId = null;    // 用于 Shift+批量选中的锚点
let visibleTextFileIds = [];     // 当前可见的文本文件 ID 列表（按渲染顺序）

const { invoke } = window.__TAURI__?.core || {};
if (!invoke) {
  console.error('Tauri API not available');
}

// ===== DOM refs =====
const $ = id => document.getElementById(id);
const treeEl = $('file-tree');
const emptyEl = $('empty-state');
const btnSettings = $('btn-settings');
const btnOpen = $('btn-open');
const btnClear = $('btn-clear');
const btnConvert = $('btn-convert');
const encodingTrigger = $('encoding-trigger');
const encodingDropdown = $('encoding-dropdown');
const encodingValue = $('encoding-value');
let currentEncoding = 'UTF-8';
const selectionCount = $('selection-count');
const statusDot = $('status-dot');
const statusText = $('status-text');
const nodeCount = $('node-count');
const overlay = $('overlay');
const dialog = $('dialog');
const dialogIcon = $('dialog-icon');
const dialogTitle = $('dialog-title');
const dialogBody = $('dialog-body');
const dialogBtn = $('dialog-btn');
const settingsDialog = $('settings-dialog');
const settingExcludeBinary = $('setting-exclude-binary');
const settingLockFiles = $('setting-lock-files');
const settingsCloseBtn = $('settings-close-btn');

// ===== Settings =====
function loadSettings() {
  try {
    const raw = localStorage.getItem('cc_settings');
    if (raw) {
      const s = JSON.parse(raw);
      if (typeof s.excludeBinary === 'boolean') {
        settingExcludeBinary.checked = s.excludeBinary;
      }
      if (typeof s.lockFiles === 'boolean') {
        settingLockFiles.checked = s.lockFiles;
      }
    }
  } catch (_) {
    // ignore
  }
}

function saveSettings() {
  try {
    localStorage.setItem('cc_settings', JSON.stringify({
      excludeBinary: settingExcludeBinary.checked,
      lockFiles: settingLockFiles.checked,
    }));
  } catch (_) {
    // ignore
  }
}

function openSettings() {
  overlay.classList.remove('hidden');
  settingsDialog.classList.remove('hidden');
}

function closeSettings() {
  overlay.classList.add('hidden');
  settingsDialog.classList.add('hidden');
}

btnSettings.addEventListener('click', openSettings);
settingsCloseBtn.addEventListener('click', closeSettings);
overlay.addEventListener('click', () => {
  if (!settingsDialog.classList.contains('hidden')) {
    closeSettings();
  }
});
settingExcludeBinary.addEventListener('change', saveSettings);
settingLockFiles.addEventListener('change', async () => {
  saveSettings();
  if (!settingLockFiles.checked) {
    try {
      await invoke('unlock_all_files');
    } catch (e) {
      console.error('解锁全部文件失败:', e);
    }
  }
});

// ===== Status =====
function setStatus(state, text) {
  statusDot.className = 'status-dot ' + state;
  statusText.textContent = text;
}

function updateCounts() {
  const total = nodes.size;
  const selected = Array.from(nodes.values()).filter(n =>
    n.is_selected && n.node_type === 'TextFile'
  ).length;
  const textFiles = Array.from(nodes.values()).filter(n =>
    n.node_type === 'TextFile'
  ).length;
  selectionCount.textContent = `已选择 ${selected}/${textFiles}`;
  nodeCount.textContent = `共 ${total} 个节点`;

  const hasSelected = selected > 0;
  btnConvert.disabled = !hasSelected || isConverting;
}

// ===== Dialog =====
function showDialog(type, title, body) {
  dialogIcon.className = 'dialog-icon ' + type;
  dialogTitle.textContent = title;
  dialogBody.textContent = body;
  overlay.classList.remove('hidden');
  dialog.classList.remove('hidden');
}

function hideDialog() {
  overlay.classList.add('hidden');
  dialog.classList.add('hidden');
}

dialogBtn.addEventListener('click', hideDialog);
overlay.addEventListener('click', hideDialog);

// ===== Tree Helpers =====
function getAllTextFileIds(dirId) {
  const result = [];
  const node = nodes.get(dirId);
  if (!node) return result;
  for (const childId of node.children) {
    const child = nodes.get(childId);
    if (!child) continue;
    if (child.node_type === 'Directory') {
      result.push(...getAllTextFileIds(childId));
    } else if (child.node_type === 'TextFile') {
      result.push(childId);
    }
  }
  return result;
}

function getDirCheckboxState(dirId) {
  const textFileIds = getAllTextFileIds(dirId);
  if (textFileIds.length === 0) return 'none';
  const selectedCount = textFileIds.filter(fid => nodes.get(fid)?.is_selected).length;
  if (selectedCount === textFileIds.length) return 'checked';
  if (selectedCount > 0) return 'indeterminate';
  return 'unchecked';
}

// ===== Tree Rendering =====
function renderTree() {
  treeEl.innerHTML = '';

  if (nodes.size === 0) {
    treeEl.appendChild(emptyEl);
    emptyEl.style.display = 'flex';
    return;
  }

  emptyEl.style.display = 'none';

  const visit = (nodeIds, depth) => {
    for (const id of nodeIds) {
      const node = nodes.get(id);
      if (!node) continue;

      const el = document.createElement('div');
      el.className = 'tree-item';
      if (node.is_selected && node.node_type === 'TextFile') {
        el.classList.add('selected');
      }
      el.dataset.id = String(id);

      const indent = document.createElement('div');
      indent.className = 'tree-indent';
      el.appendChild(indent);

      if (node.node_type === 'Directory') {
        el.classList.add('dir-item');
        indent.style.width = (depth * 20) + 'px';

        const checkbox = document.createElement('div');
        checkbox.className = 'checkbox';
        const dirState = getDirCheckboxState(id);
        if (dirState === 'checked') checkbox.classList.add('checked');
        else if (dirState === 'indeterminate') checkbox.classList.add('indeterminate');
        el.appendChild(checkbox);

        const chevron = document.createElement('div');
        chevron.className = 'chevron' + (node.is_expanded ? ' expanded' : '');
        chevron.innerHTML = `<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>`;
        el.appendChild(chevron);

        const name = document.createElement('span');
        name.className = 'dir-name';
        name.textContent = node.name;
        el.appendChild(name);
      } else {
        indent.style.width = ((depth + 1) * 20 + 10) + 'px';

        const checkbox = document.createElement('div');
        checkbox.className = 'checkbox';
        if (node.is_selected) checkbox.classList.add('checked');
        if (node.node_type === 'BinaryFile' || node.node_type === 'UnknownEncoding') {
          checkbox.classList.add('disabled');
        }
        el.appendChild(checkbox);

        const name = document.createElement('span');
        name.className = 'file-name';
        if (node.node_type === 'TextFile') name.classList.add('text-file');
        else if (node.node_type === 'BinaryFile') name.classList.add('binary-file');
        else name.classList.add('error-file');
        name.textContent = node.name;
        el.appendChild(name);

        const enc = document.createElement('span');
        if (node.conversion_error) {
          enc.className = 'file-encoding error';
          enc.textContent = node.conversion_error;
        } else if (node.node_type === 'BinaryFile') {
          enc.className = 'file-encoding';
          enc.textContent = '(binary)';
        } else if (node.node_type === 'UnknownEncoding') {
          enc.className = 'file-encoding error';
          enc.textContent = node.encoding || '未知编码';
        } else {
          enc.className = 'file-encoding';
          enc.textContent = node.encoding || 'UTF-8';
        }
        el.appendChild(enc);
      }

      // Remove button for all nodes
      const removeBtn = document.createElement('div');
      removeBtn.className = 'remove-btn';
      removeBtn.textContent = '移除';
      el.appendChild(removeBtn);

      treeEl.appendChild(el);

      if (node.node_type === 'Directory' && node.is_expanded && node.children.length > 0) {
        visit(node.children, depth + 1);
      }
    }
  };

  visit(rootNodes, 0);

  // 收集当前可见的文本文件 ID（按渲染顺序），用于 Shift+批量选中
  visibleTextFileIds = [];
  const collectVisible = (nodeIds) => {
    for (const vid of nodeIds) {
      const vNode = nodes.get(vid);
      if (!vNode) continue;
      if (vNode.node_type === 'TextFile') {
        visibleTextFileIds.push(vid);
      } else if (vNode.node_type === 'Directory' && vNode.is_expanded) {
        collectVisible(vNode.children);
      }
    }
  };
  collectVisible(rootNodes);
}

function toggleDir(id) {
  const node = nodes.get(id);
  if (!node) return;
  node.is_expanded = !node.is_expanded;
  renderTree();
}

function toggleFile(id) {
  const node = nodes.get(id);
  if (!node || node.node_type !== 'TextFile') return;
  node.is_selected = !node.is_selected;
  lastClickedFileId = id;
  renderTree();
  updateCounts();
}

function doShiftSelect(targetId) {
  const targetNode = nodes.get(targetId);
  if (!targetNode || targetNode.node_type !== 'TextFile') return;

  const anchorIndex = visibleTextFileIds.indexOf(lastClickedFileId);
  const targetIndex = visibleTextFileIds.indexOf(targetId);

  // 如果锚点已不在可见列表中（例如被移除），回退为普通 toggle
  if (anchorIndex === -1 || targetIndex === -1) {
    targetNode.is_selected = !targetNode.is_selected;
    lastClickedFileId = targetId;
    return;
  }

  const start = Math.min(anchorIndex, targetIndex);
  const end = Math.max(anchorIndex, targetIndex);

  // 目标状态：以 target 文件当前状态的反方向为准
  // 即：如果 target 当前未选中，则范围内全部选中；反之全部取消选中
  const targetState = !targetNode.is_selected;

  for (let i = start; i <= end; i++) {
    const fid = visibleTextFileIds[i];
    const f = nodes.get(fid);
    if (f && f.node_type === 'TextFile') {
      f.is_selected = targetState;
    }
  }

  lastClickedFileId = targetId;
}

function toggleDirSelect(id) {
  const node = nodes.get(id);
  if (!node || node.node_type !== 'Directory') return;
  const textFileIds = getAllTextFileIds(id);
  if (textFileIds.length === 0) return;
  const allSelected = textFileIds.every(fid => nodes.get(fid)?.is_selected);
  const newState = !allSelected;
  for (const fid of textFileIds) {
    const f = nodes.get(fid);
    if (f) f.is_selected = newState;
  }
  renderTree();
  updateCounts();
}

function removeNode(id) {
  const node = nodes.get(id);
  if (!node) return;
  // Recursively remove all children first
  if (node.node_type === 'Directory') {
    const childrenToRemove = [...node.children];
    for (const childId of childrenToRemove) {
      removeNode(childId);
    }
  }
  // Remove from parent's children list
  if (node.parent_id !== null) {
    const parent = nodes.get(node.parent_id);
    if (parent) {
      parent.children = parent.children.filter(cid => cid !== id);
    }
  } else {
    rootNodes = rootNodes.filter(rid => rid !== id);
  }
  // Remove from nodes map
  nodes.delete(id);
}

function collectTextFilePaths(startNode) {
  const paths = [];
  if (startNode.node_type === 'TextFile') {
    paths.push(startNode.path);
  } else if (startNode.node_type === 'Directory') {
    for (const childId of startNode.children) {
      const child = nodes.get(childId);
      if (child) {
        paths.push(...collectTextFilePaths(child));
      }
    }
  }
  return paths;
}

// 事件委托：所有树节点的点击统一在 treeEl 上处理
treeEl.addEventListener('click', async (e) => {
  const item = e.target.closest('.tree-item');
  if (!item) return;
  const id = parseInt(item.dataset.id, 10);
  const node = nodes.get(id);
  if (!node) return;

  // Remove button clicked
  if (e.target.closest('.remove-btn')) {
    const pathsToUnlock = collectTextFilePaths(node);
    if (pathsToUnlock.length > 0) {
      try {
        await invoke('unlock_files', { paths: pathsToUnlock });
      } catch (err) {
        console.error('解锁文件失败:', err);
      }
    }
    removeNode(id);
    renderTree();
    updateCounts();
    return;
  }

  // Checkbox clicked
  if (e.target.closest('.checkbox')) {
    if (node.node_type === 'Directory') {
      toggleDirSelect(id);
    } else if (node.node_type === 'TextFile') {
      if (e.shiftKey && lastClickedFileId !== null && lastClickedFileId !== id) {
        doShiftSelect(id);
        renderTree();
        updateCounts();
      } else {
        toggleFile(id);
      }
    }
    return;
  }

  // Chevron clicked
  if (e.target.closest('.chevron')) {
    if (node.node_type === 'Directory') {
      toggleDir(id);
    }
    return;
  }

  // Click on row itself
  if (node.node_type === 'Directory') {
    toggleDir(id);
  } else if (node.node_type === 'TextFile') {
    if (e.shiftKey && lastClickedFileId !== null && lastClickedFileId !== id) {
      doShiftSelect(id);
      renderTree();
      updateCounts();
    } else {
      toggleFile(id);
    }
  }
});

// ===== Actions =====
function remapScannedNodes(scanned) {
  let maxId = 0;
  for (const n of nodes.values()) {
    if (n.id > maxId) maxId = n.id;
  }
  const offset = maxId + 1;
  const idMap = new Map();
  for (const n of scanned) {
    idMap.set(n.id, n.id + offset);
  }
  const remapped = [];
  for (const n of scanned) {
    const newNode = { ...n };
    newNode.id = idMap.get(n.id);
    newNode.parent_id = n.parent_id !== null ? idMap.get(n.parent_id) : null;
    newNode.children = n.children.map(childId => idMap.get(childId));
    remapped.push(newNode);
  }
  return remapped;
}

function updateNodeEncoding(id, encoding) {
  const item = treeEl.querySelector(`.tree-item[data-id="${id}"]`);
  if (!item) return;
  const encEl = item.querySelector('.file-encoding');
  if (!encEl) return;
  encEl.textContent = encoding || 'UTF-8';
}

btnOpen.addEventListener('click', async () => {
  if (isScanning || isConverting) return;

  try {
    const path = await invoke('pick_directory');
    if (!path) return;

    currentPath = path;
    isScanning = true;
    btnOpen.disabled = true;
    btnConvert.disabled = true;
    setStatus('scanning', '正在扫描目录...');

    const excludeBinary = settingExcludeBinary?.checked ?? false;
    const scanned = await invoke('scan_directory', { path, excludeBinary });
    const remapped = remapScannedNodes(scanned);

    for (const n of remapped) {
      nodes.set(n.id, n);
      if (n.parent_id === null) rootNodes.push(n.id);
    }

    const detectTasks = remapped
      .filter(n => n.node_type === 'TextFile')
      .map(n => ({ id: n.id, path: n.path }));

    if (detectTasks.length > 0) {
      setStatus('detecting', `正在检测编码... (0/${detectTasks.length})`);

      const channel = new window.__TAURI__.core.Channel();
      channel.onmessage = (msg) => {
        const n = nodes.get(msg.id);
        if (n) {
          n.encoding = msg.encoding;
          updateNodeEncoding(msg.id, msg.encoding);
        }
        if (msg.completed % 50 === 0 || msg.completed === msg.total) {
          setStatus('detecting', `正在检测编码... (${msg.completed}/${msg.total})`);
        }
      };
      await invoke('detect_encodings_stream', { tasks: detectTasks, onProgress: channel });
    }

    // 锁定文本文件（如果设置开启）
    if (settingLockFiles?.checked) {
      const lockPaths = remapped
        .filter(n => n.node_type === 'TextFile')
        .map(n => n.path);
      if (lockPaths.length > 0) {
        try {
          const lockResults = await invoke('lock_files', { paths: lockPaths });
          const failures = lockResults.filter(r => !r.success);
          if (failures.length > 0) {
            console.warn('部分文件锁定失败:', failures);
            setStatus('idle', `已扫描 ${lockPaths.length} 个文本文件，${failures.length} 个无法锁定`);
          }
        } catch (e) {
          console.error('锁定文件失败:', e);
        }
      }
    }

    renderTree();
    updateCounts();
    if (!statusText.textContent?.includes('无法锁定')) {
      setStatus('idle', '就绪');
    }
  } catch (e) {
    showDialog('error', '扫描失败', String(e));
    setStatus('error', '扫描失败');
  } finally {
    isScanning = false;
    btnOpen.disabled = false;
    updateCounts();
  }
});

btnClear.addEventListener('click', async () => {
  if (isConverting) return;
  try {
    await invoke('unlock_all_files');
  } catch (e) {
    console.error('解锁全部文件失败:', e);
  }
  nodes.clear();
  rootNodes = [];
  currentPath = '';
  lastClickedFileId = null;
  renderTree();
  updateCounts();
  setStatus('idle', '就绪');
});

btnConvert.addEventListener('click', async () => {
  if (isConverting) return;

  const targetEnc = currentEncoding;
  const tasks = Array.from(nodes.values())
    .filter(n => n.is_selected && n.node_type === 'TextFile')
    .map(n => ({
      id: n.id,
      path: n.path,
      source_encoding: n.encoding || 'UTF-8',
      target_encoding: targetEnc,
      expected_size: n.file_size ?? null,
      expected_modified: n.file_modified ?? null,
    }));

  if (tasks.length === 0) {
    showDialog('error', '无文件', '未选择任何可转换的文件');
    return;
  }

  isConverting = true;
  btnConvert.disabled = true;
  btnOpen.disabled = true;
  btnClear.disabled = true;
  setStatus('converting', `正在转换 (${tasks.length} 个文件)...`);

  try {
    const results = await invoke('convert_files', {
      tasks,
      targetEncoding: targetEnc,
    });

    let successCount = 0;
    let errorCount = 0;
    for (const r of results) {
      const n = nodes.get(r.id);
      if (!n) continue;
      if (r.success) {
        successCount++;
        n.is_converting = false;
        n.conversion_error = null;
      } else {
        errorCount++;
        n.conversion_error = r.error;
      }
    }

    renderTree();

    if (errorCount === 0) {
      showDialog('success', '转换完成', `${successCount} 个文件已成功转换。`);
    } else {
      showDialog('error', '转换完成', `${successCount} 个成功，${errorCount} 个失败。`);
    }

    setStatus('idle', '就绪');
  } catch (e) {
    showDialog('error', '转换失败', String(e));
    setStatus('error', '转换失败');
  } finally {
    isConverting = false;
    btnOpen.disabled = false;
    btnClear.disabled = false;
    updateCounts();
  }
});

// ===== Encoding Dropdown =====
if (encodingTrigger && encodingDropdown) {
  encodingTrigger.addEventListener('click', (e) => {
    e.stopPropagation();
    const isOpen = !encodingDropdown.classList.contains('hidden');
    if (isOpen) {
      encodingDropdown.classList.add('hidden');
      encodingTrigger.classList.remove('open');
    } else {
      encodingDropdown.classList.remove('hidden');
      encodingTrigger.classList.add('open');
    }
  });

  document.querySelectorAll('.encoding-option').forEach(opt => {
    opt.addEventListener('click', (e) => {
      e.stopPropagation();
      const value = opt.dataset.value;
      currentEncoding = value;
      if (encodingValue) encodingValue.textContent = value;
      document.querySelectorAll('.encoding-option').forEach(o => o.classList.remove('selected'));
      opt.classList.add('selected');
      encodingDropdown.classList.add('hidden');
      encodingTrigger.classList.remove('open');
    });
  });

  document.addEventListener('click', () => {
    encodingDropdown.classList.add('hidden');
    encodingTrigger.classList.remove('open');
  });
}

// ===== Drag & Drop =====
const dropOverlay = $('drop-overlay');

function getNextId() {
  let maxId = 0;
  for (const n of nodes.values()) {
    if (n.id > maxId) maxId = n.id;
  }
  return maxId + 1;
}

function pathExists(path) {
  for (const n of nodes.values()) {
    if (n.path === path) return true;
  }
  return false;
}

async function processDroppedFiles(paths) {
  if (paths.length === 0) return;

  setStatus('scanning', '正在检测拖入的文件...');
  btnOpen.disabled = true;
  btnConvert.disabled = true;

  try {
    const results = await invoke('check_text_files', { paths });

    const textFiles = results.filter(r => r.is_text);
    const nonTextFiles = results.filter(r => !r.is_text);

    if (textFiles.length === 0) {
      let msg = '拖入的文件中没有文本文件。';
      if (nonTextFiles.length > 0) {
        msg += '\n以下文件已跳过：' + nonTextFiles.map(r => r.name).join(', ');
      }
      showDialog('error', '无有效文件', msg);
      setStatus('idle', '就绪');
      btnOpen.disabled = false;
      updateCounts();
      return;
    }

    let nextId = getNextId();
    const detectTasks = [];
    let addedCount = 0;

    const addedPaths = [];
    for (const r of textFiles) {
      if (pathExists(r.path)) continue;

      const node = {
        id: nextId++,
        name: r.name,
        path: r.path,
        node_type: 'TextFile',
        encoding: null,
        is_expanded: false,
        is_selected: true,
        is_converting: false,
        conversion_error: null,
        parent_id: null,
        children: [],
        file_size: null,
        file_modified: null,
      };
      nodes.set(node.id, node);
      rootNodes.push(node.id);
      detectTasks.push({ id: node.id, path: r.path });
      addedPaths.push(r.path);
      addedCount++;
    }

    if (detectTasks.length > 0) {
      setStatus('detecting', `正在检测编码... (0/${detectTasks.length})`);

      const channel = new window.__TAURI__.core.Channel();
      channel.onmessage = (msg) => {
        const n = nodes.get(msg.id);
        if (n) {
          n.encoding = msg.encoding;
          updateNodeEncoding(msg.id, msg.encoding);
        }
        if (msg.completed % 50 === 0 || msg.completed === msg.total) {
          setStatus('detecting', `正在检测编码... (${msg.completed}/${msg.total})`);
        }
      };
      await invoke('detect_encodings_stream', { tasks: detectTasks, onProgress: channel });
    }

    // 锁定新添加的文本文件（如果设置开启）
    if (settingLockFiles?.checked && addedPaths.length > 0) {
      try {
        const lockResults = await invoke('lock_files', { paths: addedPaths });
        const failures = lockResults.filter(r => !r.success);
        if (failures.length > 0) {
          console.warn('部分文件锁定失败:', failures);
        }
      } catch (e) {
        console.error('锁定文件失败:', e);
      }
    }

    renderTree();
    updateCounts();

    if (nonTextFiles.length > 0) {
      setStatus('idle', `已添加 ${addedCount} 个文件，跳过 ${nonTextFiles.length} 个非文本文件`);
    } else {
      setStatus('idle', `已添加 ${addedCount} 个文件`);
    }
  } catch (e) {
    showDialog('error', '添加失败', String(e));
    setStatus('error', '添加失败');
  } finally {
    btnOpen.disabled = false;
    updateCounts();
  }
}

// ===== Drag & Drop =====
let dragCounter = 0;

function showDropOverlay() {
  if (dropOverlay && !isScanning && !isConverting) {
    dropOverlay.classList.remove('hidden');
  }
}

function hideDropOverlay() {
  if (dropOverlay) {
    dropOverlay.classList.add('hidden');
  }
}

// Tauri v2 native drag-drop events (primary mechanism)
let dragOverTimer = null;

if (window.__TAURI__?.event) {
  const { listen } = window.__TAURI__.event;

  // Show overlay on drag-enter (works on macOS/Linux, sometimes on Windows)
  listen('tauri://drag-enter', () => {
    showDropOverlay();
  });

  // Hide overlay on drag-leave
  listen('tauri://drag-leave', () => {
    hideDropOverlay();
  });

  // drag-over fires continuously while hovering; use it as fallback
  // to keep overlay visible if drag-enter doesn't fire (Windows bug)
  listen('tauri://drag-over', () => {
    showDropOverlay();
    clearTimeout(dragOverTimer);
    dragOverTimer = setTimeout(hideDropOverlay, 300);
  });

  // Handle actual file drop
  listen('tauri://drag-drop', async (event) => {
    clearTimeout(dragOverTimer);
    hideDropOverlay();
    if (isScanning || isConverting) return;
    const paths = event.payload?.paths || [];
    if (paths.length > 0) {
      await processDroppedFiles(paths);
    }
  });
}

// HTML5 drag-and-drop (fallback, only works when dragDropEnabled is false)
document.addEventListener('dragenter', (e) => {
  e.preventDefault();
  dragCounter++;
  if (dragCounter === 1) showDropOverlay();
});

document.addEventListener('dragleave', (e) => {
  dragCounter--;
  if (dragCounter <= 0) {
    dragCounter = 0;
    hideDropOverlay();
  }
});

document.addEventListener('dragover', (e) => {
  e.preventDefault();
});

document.addEventListener('drop', async (e) => {
  e.preventDefault();
  dragCounter = 0;
  hideDropOverlay();
  if (isScanning || isConverting) return;

  const paths = [];
  const dtFiles = e.dataTransfer?.files;
  if (dtFiles) {
    for (let i = 0; i < dtFiles.length; i++) {
      const f = dtFiles[i];
      if (f.path) paths.push(f.path);
    }
  }
  if (paths.length === 0 && e.dataTransfer?.items) {
    for (let i = 0; i < e.dataTransfer.items.length; i++) {
      const item = e.dataTransfer.items[i];
      if (item.kind === 'file') {
        const file = item.getAsFile();
        if (file?.path) paths.push(file.path);
      }
    }
  }
  if (paths.length > 0) {
    await processDroppedFiles(paths);
  }
});

// ===== Init =====
loadSettings();
renderTree();
updateCounts();
