export const buttonVariants = {
  primary: { label: 'Primary', className: 'primary-button' },
  secondary: { label: 'Secondary', className: 'secondary-action' },
  soft: { label: 'Soft', className: 'text-button' },
  danger: { label: 'Danger', className: 'danger-button' }
} as const;

export type ButtonVariantName = keyof typeof buttonVariants;

export function resolveButtonVariant(value: string | null): ButtonVariantName {
  return value !== null && value in buttonVariants ? (value as ButtonVariantName) : 'primary';
}
