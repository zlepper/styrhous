<script lang="ts">
  import { buttonVariants } from '$lib/buttonStyles';
  import arrowLeftIcon from '../../../../../../crates/components/src/icons/arrow-left.svg?url&no-inline';
  import arrowRightIcon from '../../../../../../crates/components/src/icons/arrow-right.svg?url&no-inline';
  import ellipsisIcon from '../../../../../../crates/components/src/icons/ellipsis-horizontal.svg?url&no-inline';
  import pencilIcon from '../../../../../../crates/components/src/icons/pencil.svg?url&no-inline';
  import trashIcon from '../../../../../../crates/components/src/icons/trash.svg?url&no-inline';
  import '../showcase.css';

  const columns = [
    { label: 'XS', left: 351, center: 381, sizeClass: 'button-size-xs' },
    { label: 'SM', left: 545, center: 575, sizeClass: 'button-size-sm' },
    { label: 'MD', left: 757, center: 787, sizeClass: 'button-size-md' },
    { label: 'LG', left: 982, center: 1012, sizeClass: 'button-size-lg' },
    { label: 'XL', left: 1221, center: 1251, sizeClass: 'button-size-xl' }
  ] as const;

  const rows = [
    { ...buttonVariants.primary, labelTop: 310, centerY: 321 },
    { ...buttonVariants.secondary, labelTop: 412, centerY: 423 },
    { ...buttonVariants.soft, labelTop: 514, centerY: 525 },
    { ...buttonVariants.danger, labelTop: 610, centerY: 621 }
  ] as const;

  const iconButtons = [
    { label: 'Back', source: arrowLeftIcon, center: 178 },
    { label: 'Forward', source: arrowRightIcon, center: 296 },
    { label: 'Edit', source: pencilIcon, center: 411 },
    { label: 'Delete', source: trashIcon, center: 525 },
    { label: 'More actions', source: ellipsisIcon, center: 630 }
  ] as const;

  const roundingButtons = [
    { label: 'Default', center: 886, className: '' },
    { label: 'Rounded', center: 1076, className: 'button-rounding-rounded' },
    { label: 'Pill', center: 1261, className: 'button-rounding-pill' }
  ] as const;

</script>

<svelte:head>
  <title>Button showcase · Styrhous</title>
</svelte:head>

<div class="component-showcase button-showcase" data-testid="button-showcase">
  <h1 class="button-showcase-title">Buttons</h1>
  <p class="button-showcase-subtitle">Application button styles and sizes</p>
  <div class="button-showcase-card" aria-hidden="true"></div>

  {#each columns as column}
    <span class="showcase-column-label" style={`--left: ${column.left}px`}>{column.label}</span>
  {/each}

  {#each rows as row}
    <span class="showcase-row-label" style={`--top: ${row.labelTop}px`}>{row.label}</span>
    {#each columns as column}
      <button
        class={`showcase-button ${row.className} ${column.sizeClass}`}
        data-size={column.label.toLowerCase()}
        data-variant={row.label.toLowerCase()}
        style={`--center-y: ${row.centerY}px; --center-x: ${column.center}px`}
        type="button"
      >Button text</button>
    {/each}
  {/each}

  <div class="showcase-divider-horizontal" aria-hidden="true"></div>
  <div class="showcase-divider-vertical" aria-hidden="true"></div>

  <h2 class="showcase-section-title" style="--left: 140px">Icon buttons</h2>
  {#each iconButtons as iconButton}
    <button
      class="showcase-button showcase-icon-button secondary-action button-size-xl"
      style={`--center-y: 835px; --center-x: ${iconButton.center}px`}
      type="button"
      aria-label={iconButton.label}
    >
      <img src={iconButton.source} alt="" />
    </button>
  {/each}

  <h2 class="showcase-section-title" style="--left: 818px">Corner radius</h2>
  {#each roundingButtons as roundingButton}
    <button
      class={`showcase-button primary-button button-size-xl ${roundingButton.className}`}
      style={`--center-y: 838px; --center-x: ${roundingButton.center}px`}
      type="button"
    >{roundingButton.label}</button>
  {/each}
</div>
