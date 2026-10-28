// The Files window: two trees, either of which can be this machine or a
// connected host, and the copies between them underneath (filesview.js).
import { invoke, listen, startPage, pageReady, t, commonKeys, setTitle } from './shellder.js';
import { createFilesView } from './filesview.js';

const host = new URLSearchParams(location.search).get('host');

async function init() {
  const app = await startPage();
  setTitle(t('Files — {host}', { host }));
  const view = createFilesView(document.getElementById('files'), { host, transferHeight: app.transferPanelHeight || 160 });
  pageReady();
  await listen(`files:files-${host.replace(/[^A-Za-z0-9_-]/g, '_')}`, view.render);
  view.render(await invoke('files_open', { host }));
  view.nativeDrops();
  commonKeys();
}
init();
