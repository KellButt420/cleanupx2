import { latestReleaseUrl, readConfig, wingetCommand } from './config.ts';
import release from '../../release.json';
import { setupMotion } from './motion.ts';

const config = readConfig(import.meta.env, release);
const $ = <T extends Element = HTMLElement>(selector: string, root: ParentNode = document) => root.querySelector<T>(selector);
const $$ = <T extends Element = HTMLElement>(selector: string, root: ParentNode = document) => [...root.querySelectorAll<T>(selector)];
const setText = (selector: string, text: string) => {
  const element = $(selector);
  if (element) element.textContent = text;
};

// Compact navigation uses the same destinations as the desktop header.
const menuToggle = $<HTMLButtonElement>('.menu-toggle');
const navigation = $('#main-navigation');
function closeMenu() {
  menuToggle?.setAttribute('aria-expanded', 'false');
  menuToggle?.setAttribute('aria-label', 'Open navigation');
  navigation?.classList.remove('is-open');
}
menuToggle?.addEventListener('click', () => {
  const open = menuToggle.getAttribute('aria-expanded') !== 'true';
  menuToggle.setAttribute('aria-expanded', String(open));
  menuToggle.setAttribute('aria-label', open ? 'Close navigation' : 'Open navigation');
  navigation?.classList.toggle('is-open', open);
});
navigation?.addEventListener('click', (event) => {
  if (event.target instanceof Element && event.target.closest('a')) closeMenu();
});
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape' && menuToggle?.getAttribute('aria-expanded') === 'true') {
    closeMenu();
    menuToggle.focus();
  }
});
document.addEventListener('click', (event) => {
  if (event.target instanceof Node && !menuToggle?.contains(event.target) && !navigation?.contains(event.target)) closeMenu();
});
matchMedia('(min-width: 768px)').addEventListener('change', closeMenu);

// Tabs: shared by the preview rail and the install panel (WAI-ARIA tabs pattern).
function setupTabs(tabs: HTMLButtonElement[]) {
  const select = (tab: HTMLButtonElement, focus = false) => {
    for (const other of tabs) {
      const active = other === tab;
      other.setAttribute('aria-selected', String(active));
      other.tabIndex = active ? 0 : -1;
      const panel = document.getElementById(other.getAttribute('aria-controls') ?? '');
      if (panel) panel.hidden = !active;
    }
    if (focus) tab.focus();
  };

  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => select(tab));
    tab.addEventListener('keydown', (event) => {
      const vertical = tab.parentElement?.getAttribute('aria-orientation') === 'vertical';
      const next = vertical ? 'ArrowDown' : 'ArrowRight';
      const previous = vertical ? 'ArrowUp' : 'ArrowLeft';
      const target = {
        [next]: tabs[(index + 1) % tabs.length],
        [previous]: tabs[(index - 1 + tabs.length) % tabs.length],
        Home: tabs[0],
        End: tabs.at(-1),
      }[event.key];
      if (!target) return;
      event.preventDefault();
      select(target, true);
    });
  });

  return select;
}

const selectView = setupTabs($$<HTMLButtonElement>('.app-tabs [role="tab"]'));
setupTabs($$<HTMLButtonElement>('.install-tabs [role="tab"]'));

// Keep keyboard navigation aligned with the preview's container-based layout.
const previewWindow = $('.app-window');
const previewTabs = $('.app-tabs');
if (previewWindow) {
  new ResizeObserver(([entry]) => {
    if (entry) previewTabs?.setAttribute('aria-orientation', entry.contentRect.width <= 520 ? 'horizontal' : 'vertical');
  }).observe(previewWindow);
}

for (const link of $$<HTMLAnchorElement>('[data-show-view]')) {
  link.addEventListener('click', () => {
    const tab = $<HTMLButtonElement>(`.app-tabs [data-view="${link.dataset.showView}"]`);
    if (tab) selectView(tab);
  });
}

// Example application list: search, selection, and the selection summary in the rail and the compact footer.
const rows = $$<HTMLLabelElement>('#application-list .app-row');
const search = $<HTMLInputElement>('#app-search');
const count = $('.app-count');
const empty = $('.empty-search');
const summaries = $$('[data-selection-summary]');
const reviewButtons = $$<HTMLButtonElement>('[data-review]');
const selectionActive = $('[data-selection-active]');
const selectionEmpty = $('[data-selection-empty]');
const selectionIcons = $('#selection-icons');
const dialog = $<HTMLDialogElement>('#selection-dialog');
const dialogList = $('#dialog-selection');

const checkedRows = () => rows.filter((row) => $<HTMLInputElement>('input', row)?.checked);
const selectedNames = () => checkedRows().map((row) => $<HTMLInputElement>('input', row)?.value ?? '');

// As in the app: up to six selected icons, then +N for the rest.
function renderSelection() {
  const selected = checkedRows();
  const total = selected.length;
  const summary = total === 0 ? 'You choose what goes.' : `${total} selected · Nothing removed yet.`;
  for (const element of summaries) element.textContent = summary;
  for (const button of reviewButtons) button.disabled = total === 0;
  if (selectionActive) selectionActive.hidden = total === 0;
  if (selectionEmpty) selectionEmpty.hidden = total > 0;
  selectionIcons?.replaceChildren(
    ...selected.slice(0, 6).map((row) => $('.sample-icon', row)!.cloneNode(true) as HTMLElement),
    ...(total > 6 ? [Object.assign(document.createElement('span'), { className: 'sel-more', textContent: `+${total - 6}` })] : []),
  );
}

search?.addEventListener('input', () => {
  const query = search.value.trim().toLowerCase();
  let visible = 0;
  for (const row of rows) {
    const match = !query || (row.dataset.name ?? '').includes(query);
    row.hidden = !match;
    if (match) visible++;
  }
  if (count) count.textContent = String(visible);
  if (empty) empty.hidden = visible > 0;
});

$('#application-list')?.addEventListener('change', renderSelection);

$('#clear-selection')?.addEventListener('click', () => {
  for (const row of rows) {
    const input = $<HTMLInputElement>('input', row);
    if (input) input.checked = false;
  }
  renderSelection();
});

for (const button of reviewButtons) {
  button.addEventListener('click', () => {
    if (!dialog || !dialogList) return;
    dialogList.replaceChildren(...selectedNames().map((name) => Object.assign(document.createElement('li'), { textContent: name })));
    dialog.showModal();
  });
}

renderSelection();

dialog?.addEventListener('click', (event) => {
  if (event.target === dialog) dialog.close();
});

// Organize: a finished move offers Undo in place.
const move = $<HTMLButtonElement>('#organize-move');
const result = $('#organize-result');
move?.addEventListener('click', () => {
  if (result) result.hidden = false;
  if (move) move.disabled = true;
});
$('#organize-undo')?.addEventListener('click', () => {
  if (result) result.hidden = true;
  if (move) move.disabled = false;
});

// Theme.
const themeToggle = $<HTMLButtonElement>('.theme-toggle');
const themeIcon = $<SVGUseElement>('.theme-toggle use');
const themeColor = $<HTMLMetaElement>('meta[name="theme-color"]');

function applyTheme(dark: boolean) {
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  themeIcon?.setAttribute('href', dark ? '#i-sun' : '#i-moon');
  themeToggle?.setAttribute('aria-label', dark ? 'Switch to light theme' : 'Switch to dark theme');
  themeColor?.setAttribute('content', dark ? '#17151e' : '#fcfbfe');
}

applyTheme(document.documentElement.dataset.theme === 'dark');
themeToggle?.addEventListener('click', () => {
  const dark = document.documentElement.dataset.theme !== 'dark';
  applyTheme(dark);
  try { localStorage.setItem('cleanup-theme', dark ? 'dark' : 'light'); } catch { /* private mode */ }
});

// Release links, only when configured.
if (config.downloadUrl) {
  const link = $<HTMLAnchorElement>('#download-link');
  if (link) {
    link.href = config.downloadUrl;
    link.textContent = 'Download for Windows';
  }
  setText('#download-heading', 'Ready when you are.');
  setText('#download-description', 'Download the signed MSIX package for Windows 11 x64 and open it to install.');
}

if (config.wingetId) {
  const command = wingetCommand(config.wingetId);
  const line = $('.command-line');
  const code = $('#winget-command');
  const copy = $<HTMLButtonElement>('#copy-winget');
  const copyStatus = $('#copy-status');
  if (code) code.textContent = command;
  if (line) line.hidden = false;
  setText('#winget-description', 'Install from the WinGet community repository. Updates arrive with winget upgrade.');
  setText('#winget-faq', `Yes. Run ${command} in a terminal.`);
  copy?.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(command);
      if (copyStatus) copyStatus.textContent = 'Copied to clipboard.';
    } catch {
      if (copyStatus) copyStatus.textContent = 'Copy failed. Select the command to copy it.';
    }
  });
}

if (config.repositoryUrl) {
  for (const link of $$<HTMLAnchorElement>('[data-repository]')) {
    link.href = config.repositoryUrl;
    link.hidden = false;
  }
  setText('#source-faq', 'Yes. cleanup is MIT licensed and its source is public on the project repository.');
  setText('#source-description', 'Native C# and WinUI on the surface. Rust for the system work. MIT licensed, with the source on GitHub.');
}

for (const [selector, url] of [
  ['[data-discord]', config.discordUrl],
  ['[data-releases]', latestReleaseUrl(config.repositoryUrl)?.replace(/\/latest$/, config.downloadUrl || config.appInstallerUrl ? '/latest' : '') ?? null],
] as const) {
  if (!url) continue;
  for (const link of $$<HTMLAnchorElement>(selector)) {
    link.href = url;
    link.hidden = false;
  }
}
if (config.appInstallerUrl) {
  const link = $<HTMLAnchorElement>('#download-link');
  if (link) { link.href = config.appInstallerUrl; link.textContent = 'Install for Windows'; }
  setText('#download-heading', 'Ready when you are.');
  setText('#download-description', 'Open the App Installer file to install cleanup. Windows checks for signed updates when you launch it.');
  setText('#download-footnote', 'Windows 11 x64. Updates preserve your settings and history.');
} else if (config.downloadUrl) {
  setText('#download-footnote', 'Windows 11 x64. Download the signed package from the release.');
}

setupMotion();
