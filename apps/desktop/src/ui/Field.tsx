import {
  forwardRef,
  useId,
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
  const classes = ['ds-field', inline ? 'ds-field--inline' : ''].filter(Boolean).join(' ');
  return (
    <div className={classes}>
      {label && (
        <label className="ds-field-label" htmlFor={htmlFor}>
          {label}
        </label>
      )}
      {children}
      {hint && !error && <span className="ds-field-hint">{hint}</span>}
      {error && <span className="ds-field-error">{error}</span>}
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
        className={['ds-input', className ?? ''].filter(Boolean).join(' ')}
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
        className={['ds-select', className ?? ''].filter(Boolean).join(' ')}
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
          className={['ds-textarea', className ?? ''].filter(Boolean).join(' ')}
          {...rest}
        />
      </FieldShell>
    );
  },
);
