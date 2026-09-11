<script lang="ts">
  import { page } from '$app/state';
  import { buttonVariants, resolveButtonVariant } from '$lib/buttonStyles';
  import '../showcase.css';

  const variantName = $derived(resolveButtonVariant(page.url.searchParams.get('variant')));
  const interactionState = $derived(page.url.searchParams.get('state') ?? 'hovered');
  const interaction = $derived(
    interactionState === 'pressed'
      ? 'Pressed'
      : interactionState === 'focused'
        ? 'Focused'
        : interactionState === 'disabled'
          ? 'Disabled'
          : 'Hovered'
  );
  const variant = $derived(buttonVariants[variantName]);
  const interactionWidth = $derived(interactionState === 'pressed' ? 105.7 : 108.1);
  const interactionSurfaceWidth = $derived(interactionState === 'pressed' ? 213 : 216);
</script>

<svelte:head>
  <title>Button interaction showcase · Styrhous</title>
</svelte:head>

<div
  class="component-showcase transparent-showcase interaction-showcase"
  data-testid="button-interaction-showcase"
  style={`--interaction-surface-width: ${interactionSurfaceWidth}px`}
>
  <p>{variant.label}: Default vs {interaction}</p>
  <div class="interaction-buttons">
    <button class={variant.className} type="button">Default</button>
    <button
      class={variant.className}
      type="button"
      disabled={interactionState === 'disabled'}
      style={`width: ${interactionWidth}px`}
    >{interaction}</button>
  </div>
</div>
