<script lang="ts" module>
  // One 20px stroke icon set for the whole admin (no emoji, no per-page paths).
  const PATHS = {
    receive: '<path d="M3 11.5V15a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 17 15v-3.5M3 11.5h3.6l1 1.8h4.8l1-1.8H17M10 3.5v6.2m0 0L7.4 7.2M10 9.7l2.6-2.5"/>',
    send: '<path d="M3 11.5V15a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 17 15v-3.5M3 11.5h3.6l1 1.8h4.8l1-1.8H17M10 9.7V3.5m0 0L7.4 6M10 3.5L12.6 6"/>',
    up: '<path d="M10 15.5V4.5m0 0L5.5 9M10 4.5L14.5 9"/>',
    down: '<path d="M10 4.5v11m0 0L5.5 11m4.5 4.5l4.5-4.5"/>',
    copy: '<rect x="7" y="7" width="9" height="9" rx="1.5"/><path d="M13 7V5.5A1.5 1.5 0 0 0 11.5 4h-6A1.5 1.5 0 0 0 4 5.5v6A1.5 1.5 0 0 0 5.5 13H7"/>',
    open: '<path d="M8 5H5.5A1.5 1.5 0 0 0 4 6.5v8A1.5 1.5 0 0 0 5.5 16h8a1.5 1.5 0 0 0 1.5-1.5V12M11 4h5v5M16 4l-7 7"/>',
    revoke: '<circle cx="10" cy="10" r="6.2"/><path d="M5.6 5.6l8.8 8.8"/>',
    folder: '<path d="M3 6.2c0-.7.5-1.2 1.2-1.2h3.4l1.6 1.7h6.6c.7 0 1.2.5 1.2 1.2v6.9c0 .7-.5 1.2-1.2 1.2H4.2c-.7 0-1.2-.5-1.2-1.2z"/>',
    project: '<rect x="3" y="4" width="14" height="12" rx="2"/><path d="M3 8h14"/>',
    team: '<circle cx="8" cy="7.5" r="2.6"/><path d="M3.5 15.5c.6-2.4 2.3-3.6 4.5-3.6s3.9 1.2 4.5 3.6M13 5.2a2.4 2.4 0 0 1 0 4.6M14.5 11.9c1.3.5 2.1 1.6 2.4 3.6"/>',
    file: '<path d="M6 3.5h5.5L15 7v8.5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1zM11.5 3.5V7H15"/>',
    chevron: '<path d="M7 8l3 3 3-3"/>',
    "chevron-up": '<path d="M7 12l3-3 3 3"/>',
    "chevron-right": '<path d="M8 7l3 3-3 3"/>',
    search: '<circle cx="9" cy="9" r="5"/><path d="M13 13l3.5 3.5"/>',
    x: '<path d="M6 6l8 8M14 6l-8 8"/>',
    lock: '<rect x="5" y="9" width="10" height="7.5" rx="1.5"/><path d="M7 9V7a3 3 0 0 1 6 0v2"/>',
    mail: '<rect x="3" y="5" width="14" height="10" rx="1.5"/><path d="M3.5 6l6.5 5 6.5-5"/>',
    sync: '<path d="M15.5 8A5.6 5.6 0 0 0 5 6.6M4.5 12A5.6 5.6 0 0 0 15 13.4M5 3.6v3h3M15 16.4v-3h-3"/>',
    link: '<path d="M8.5 11.5l3-3M7.2 9.3l-1.6 1.6a2.6 2.6 0 0 0 3.7 3.7l1.6-1.6M12.8 10.7l1.6-1.6a2.6 2.6 0 0 0-3.7-3.7L9.1 7"/>',
    retry: '<path d="M15.5 10a5.5 5.5 0 1 1-1.6-3.9M15.5 4v3.4h-3.4"/>',
    trash: '<path d="M4.5 6h11M8 6V4.5h4V6M6 6l.7 9.5h6.6L14 6"/>',
    pause: '<path d="M7.5 5v10M12.5 5v10"/>',
    play: '<path d="M7 5.5v9l7-4.5z"/>',
    pencil: '<path d="M12.8 4.7l2.5 2.5M4 16l.7-3.2 8.6-8.6a1.2 1.2 0 0 1 1.7 0l.8.8a1.2 1.2 0 0 1 0 1.7l-8.6 8.6z"/>',
    history: '<path d="M3.5 10a6.5 6.5 0 1 0 1.9-4.6M3.5 4v3.4h3.4M10 6.5V10l2.5 1.6"/>',
    sliders: '<path d="M4 6h7m4 0h1M4 14h1m4 0h7"/><circle cx="13" cy="6" r="2"/><circle cx="7" cy="14" r="2"/>',
    check: '<path d="M5 10.5l3.2 3L15 6.5"/>',
    home: '<path d="M3.5 9.5L10 4l6.5 5.5M5.5 8.4V16h3.4v-4h2.2v4h3.4V8.4"/>',
    plus: '<path d="M10 4.5v11M4.5 10h11"/>',
    clock: '<circle cx="10" cy="10" r="6.5"/><path d="M10 6.5V10l2.3 1.5"/>',
    warning: '<path d="M10 4l6.5 11.5h-13zM10 9v3M10 14.2v.1"/>',
    nas: '<rect x="3.5" y="4" width="13" height="5" rx="1.2"/><rect x="3.5" y="11" width="13" height="5" rx="1.2"/><path d="M6 6.5h.01M6 13.5h.01"/>',
    move: '<path d="M4 10h12m0 0l-3.5-3.5M16 10l-3.5 3.5"/>',
    more: '<circle cx="5" cy="10" r="0.6"/><circle cx="10" cy="10" r="0.6"/><circle cx="15" cy="10" r="0.6"/>',
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let {
    name,
    size = 16,
    class: klass = "",
    title = undefined,
  }: { name: IconName; size?: number; class?: string; title?: string } = $props();
</script>

<svg
  viewBox="0 0 20 20"
  width={size}
  height={size}
  class="shrink-0 {klass}"
  fill="none"
  stroke="currentColor"
  stroke-width={name === "more" ? 2.4 : 1.5}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden={title ? undefined : "true"}
  role={title ? "img" : undefined}
>
  {#if title}<title>{title}</title>{/if}
  {@html PATHS[name]}
</svg>
