// [B17] Atelier v2 — .field + .field-label + .input/.input--box per wotold.css.
// Boxed-вариант (.input--box) — для multi-row settings форм; bare (.input)
// — для одиночных editorial-инпутов (search / hero).

import {
  forwardRef,
  useId,
  type CSSProperties,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from 'react';

interface FieldShellProps {
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  inline?: boolean;
  htmlFor: string;
  children: ReactNode;
}

function FieldShell({ label, hint, error, inline, htmlFor, children }: FieldShellProps) {
  const containerStyle: CSSProperties = inline
    ? {
        display: 'flex',
        flexDirection: 'row',
        alignItems: 'center',
        gap: 12,
        margin: 'var(--s2) 0',
      }
    : { display: 'flex', flexDirection: 'column', gap: 6, margin: 'var(--s2) 0' };
  return (
    <div className="field" style={containerStyle}>
      {label && (
        <label className="field-label" htmlFor={htmlFor}>
          {label}
        </label>
      )}
      {children}
      {hint && !error && (
        <span style={{ fontSize: 12, color: 'var(--text-faint)', marginTop: 2 }}>
          {hint}
        </span>
      )}
      {error && (
        <span
          style={{
            fontSize: 12,
            color: 'var(--danger)',
            fontFamily: 'var(--mono)',
            marginTop: 2,
          }}
        >
          {error}
        </span>
      )}
    </div>
  );
}

interface InputFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
}

export const InputField = forwardRef<HTMLInputElement, InputFieldProps>(function InputField(
  { id, label, hint, error, className, ...rest },
  ref,
) {
  const autoId = useId();
  const fieldId = id ?? autoId;
  return (
    <FieldShell htmlFor={fieldId} label={label} hint={hint} error={error}>
      <input
        ref={ref}
        id={fieldId}
        className={['input', 'input--box', className ?? ''].filter(Boolean).join(' ')}
        {...rest}
      />
    </FieldShell>
  );
});

interface SelectFieldProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
}

export const SelectField = forwardRef<HTMLSelectElement, SelectFieldProps>(function SelectField(
  { id, label, hint, error, className, children, ...rest },
  ref,
) {
  const autoId = useId();
  const fieldId = id ?? autoId;
  return (
    <FieldShell htmlFor={fieldId} label={label} hint={hint} error={error}>
      <select
        ref={ref}
        id={fieldId}
        className={['input', 'input--box', className ?? ''].filter(Boolean).join(' ')}
        style={{ fontFamily: 'var(--font)' }}
        {...rest}
      >
        {children}
      </select>
    </FieldShell>
  );
});

interface TextareaFieldProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
}

export const TextareaField = forwardRef<HTMLTextAreaElement, TextareaFieldProps>(
  function TextareaField({ id, label, hint, error, className, ...rest }, ref) {
    const autoId = useId();
    const fieldId = id ?? autoId;
    return (
      <FieldShell htmlFor={fieldId} label={label} hint={hint} error={error}>
        <textarea
          ref={ref}
          id={fieldId}
          className={['input', 'input--box', className ?? ''].filter(Boolean).join(' ')}
          style={{
            resize: 'vertical',
            minHeight: '4rem',
            fontFamily: 'var(--font)',
          }}
          {...rest}
        />
      </FieldShell>
    );
  },
);
