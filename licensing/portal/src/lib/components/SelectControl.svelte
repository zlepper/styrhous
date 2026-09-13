<script lang="ts">
  import type { Snippet } from 'svelte';
  import chevronDownIcon from '../../../../../crates/components/src/icons/chevron-down.svg?url&no-inline';

  let { children }: { children: Snippet } = $props();
</script>

<span class="select-control">
  {@render children()}
  <img class="select-control-chevron" src={chevronDownIcon} alt="" aria-hidden="true" />
</span>
