// Button: React Aria-backed button atom. The exported API stays the original
// native-shaped contract (variant/size/icon/onClick + full button attributes)
// so existing call sites keep compiling; useButton supplies the press
// semantics, disabled handling, and focus behavior of the Spectrum component
// family while this package owns the rendered element — every native
// attribute (title, data-*, handlers) lands on the real <button>. Visuals
// stay token-driven through the same .module.css classes as before.

import { useButton } from '@react-aria/button'
import type { ButtonHTMLAttributes, ReactNode, RefObject } from 'react'
import { useRef } from 'react'
import clsx from 'clsx'
import css from './Button.module.css'

/** Visual variant, each backed by its --dsw-alias-button-* token family. */
export type ButtonVariant = 'primary' | 'ghost' | 'outline' | 'toolbar'

/**
 * Render a pressable button under the DeepSeek token palette.
 * @param props.variant - visual family (default 'ghost').
 * @param props.size - 'md' 36px capsule (figma Button) or 'sm' 28px compact.
 * @param props.icon - optional leading 16px icon node.
 * @returns the button element; native attributes pass through unchanged.
 */
export function Button({ variant = 'ghost', size = 'md', icon, className, children, ...rest }: {
  variant?: ButtonVariant
  size?: 'md' | 'sm'
  icon?: ReactNode
  className?: string | undefined
  children?: ReactNode
} & ButtonHTMLAttributes<HTMLButtonElement>) {
  const ref = useRef<HTMLButtonElement>(null)
  // Press behavior stays native: onClick rides the element below, while
  // useButton supplies disabled semantics, focus ring wiring, and keyboard
  // activation parity with the Spectrum component.
  const { buttonProps } = useButton({
    type: 'button',
    isDisabled: rest.disabled === true,
  }, ref as RefObject<HTMLButtonElement>)
  return (
    <button
      {...buttonProps}
      {...rest}
      className={clsx(css.button, css[variant], css[size], className)}
      ref={ref}
    >
      {icon != null && <span className={css.icon}>{icon}</span>}
      {children}
    </button>
  )
}
