<script lang="ts">
  let {
    checked = $bindable(false),
    label,
    disabled = false,
    id = undefined,
    onchange = undefined,
  }: { checked?: boolean; label: string; disabled?: boolean; id?: string; onchange?: (v: boolean) => void } = $props();
</script>

<button
  type="button"
  role="switch"
  {id}
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => {
    checked = !checked;
    onchange?.(checked);
  }}
  class="relative h-[19px] w-8 shrink-0 rounded-full border transition-colors disabled:opacity-40 {checked
    ? 'border-transparent bg-accent'
    : 'border-border-strong bg-surface-3'}"
>
  <span
    class="absolute top-[2px] h-[13px] w-[13px] rounded-full transition-[left] {checked ? 'left-[15px] bg-on-accent' : 'left-[2px] bg-muted'}"
  ></span>
</button>
