<script lang="ts">
  // A destructive action that confirms in place: the first click turns the button into a red
  // "Remove?"-style button for 3 seconds; a second click within that time runs the action.
  // Replaces the browser's confirm() dialog so attention stays on the row.
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    label,
    confirmLabel = undefined,
    icon = undefined,
    onconfirm,
    disabled = false,
    title = undefined,
    class: klass = "",
  }: {
    // Shown before the first click. With `icon`, the button is icon-only and `label` is its name.
    label: string;
    // Shown while armed (defaults to `label`).
    confirmLabel?: string;
    icon?: IconName;
    onconfirm: () => unknown;
    disabled?: boolean;
    title?: string;
    class?: string;
  } = $props();

  let armed = $state(false);
  let busy = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function click(e: MouseEvent) {
    e.stopPropagation();
    if (!armed) {
      armed = true;
      clearTimeout(timer);
      timer = setTimeout(() => (armed = false), 3000);
      return;
    }
    clearTimeout(timer);
    armed = false;
    busy = true;
    try {
      await onconfirm();
    } finally {
      busy = false;
    }
  }
</script>

{#if armed}
  <button
    type="button"
    onclick={click}
    class="inline-flex h-7 items-center rounded-md bg-danger/15 px-2.5 text-xs font-medium whitespace-nowrap text-danger hover:bg-danger/25 {klass}"
  >{confirmLabel ?? label}</button>
{:else if icon}
  <button
    type="button"
    onclick={click}
    disabled={disabled || busy}
    aria-label={label}
    title={title ?? label}
    class="inline-grid h-7 w-7 place-items-center rounded-md text-faint hover:bg-surface-3 hover:text-text disabled:opacity-40 {klass}"
  ><Icon name={icon} /></button>
{:else}
  <button
    type="button"
    onclick={click}
    disabled={disabled || busy}
    title={title}
    class="inline-flex h-7 items-center rounded-md px-2.5 text-xs font-medium whitespace-nowrap text-muted hover:bg-surface-3 hover:text-danger disabled:opacity-40 {klass}"
  >{label}</button>
{/if}
