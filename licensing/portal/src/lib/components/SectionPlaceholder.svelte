<script lang="ts">
  let {
    eyebrow,
    heading,
    description
  }: { eyebrow: string; heading: string; description: string } = $props();
</script>

<section class="page-heading" aria-labelledby="section-heading">
  <p class="eyebrow">{eyebrow}</p>
  <h1 id="section-heading">{heading}</h1>
  <p class="lede">{description}</p>
</section>

<section class="empty-state" aria-labelledby="foundation-heading">
  <span class="empty-state-mark" aria-hidden="true">S</span>
  <div>
    <h2 id="foundation-heading">Ready for account data</h2>
    <p>This section will populate after portal authentication and API integration are connected.</p>
  </div>
</section>
