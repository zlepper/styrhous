<script lang="ts">
  import type { Snippet } from 'svelte';
  import ellipsisIcon from '../../../../../crates/components/src/icons/ellipsis-horizontal.svg?url&no-inline';

  let {
    id,
    subject,
    disabled = false,
    children
  }: {
    id: string;
    subject: string;
    disabled?: boolean;
    children: Snippet;
  } = $props();

  function focusFirstAction(event: ToggleEvent) {
    if (event.newState !== 'open') return;
    const menu = event.currentTarget as HTMLElement;
    requestAnimationFrame(() =>
      menu.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus()
    );
  }

</script>

<div class="organization-seat-actions">
  <button
    class="secondary-action organization-seat-menu-trigger"
    type="button"
    {disabled}
    popovertarget={id}
    aria-label={`More actions for ${subject}`}
    title={`More actions for ${subject}`}
  >
    <img src={ellipsisIcon} alt="" />
  </button>
  <div
    class="organization-seat-menu"
    {id}
    popover="auto"
    role="group"
    aria-label={`Actions for ${subject}`}
    ontoggle={focusFirstAction}
  >
    {@render children()}
  </div>
</div>
