import { useId } from "react";
import type {
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";

interface FieldShellProps {
  label: string;
  required?: boolean;
  error?: string | undefined;
  hint?: string;
}

interface ControlIds {
  id: string;
  describedBy: string | undefined;
}

function FieldShell({
  label,
  required = false,
  error,
  hint,
  render,
}: FieldShellProps & { render: (ids: ControlIds) => ReactNode }) {
  const id = useId();
  const messageId = `${id}-message`;
  const hasMessage = error !== undefined || hint !== undefined;
  return (
    <div className={error === undefined ? "ui-field" : "ui-field ui-field--invalid"}>
      <label className="ui-field__label" htmlFor={id}>
        {label}
        {required ? (
          <span className="ui-field__required" aria-hidden="true">
            *
          </span>
        ) : null}
      </label>
      {render({ id, describedBy: hasMessage ? messageId : undefined })}
      {error !== undefined ? (
        <p className="ui-field__error" id={messageId} role="alert">
          {error}
        </p>
      ) : hint !== undefined ? (
        <p className="ui-field__hint" id={messageId}>
          {hint}
        </p>
      ) : null}
    </div>
  );
}

export type InputFieldProps = FieldShellProps &
  Omit<InputHTMLAttributes<HTMLInputElement>, "id" | "required">;

export function InputField({ label, required, error, hint, ...rest }: InputFieldProps) {
  return (
    <FieldShell
      label={label}
      {...(required === undefined ? {} : { required })}
      error={error}
      {...(hint === undefined ? {} : { hint })}
      render={({ id, describedBy }) => (
        <input
          {...rest}
          id={id}
          className="ui-field__control"
          required={required ?? false}
          aria-invalid={error !== undefined}
          aria-describedby={describedBy}
        />
      )}
    />
  );
}

export type TextareaFieldProps = FieldShellProps &
  Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "id" | "required">;

export function TextareaField({ label, required, error, hint, ...rest }: TextareaFieldProps) {
  return (
    <FieldShell
      label={label}
      {...(required === undefined ? {} : { required })}
      error={error}
      {...(hint === undefined ? {} : { hint })}
      render={({ id, describedBy }) => (
        <textarea
          {...rest}
          id={id}
          className="ui-field__control"
          required={required ?? false}
          aria-invalid={error !== undefined}
          aria-describedby={describedBy}
        />
      )}
    />
  );
}

export type SelectFieldProps = FieldShellProps &
  Omit<SelectHTMLAttributes<HTMLSelectElement>, "id" | "required">;

export function SelectField({ label, required, error, hint, children, ...rest }: SelectFieldProps) {
  return (
    <FieldShell
      label={label}
      {...(required === undefined ? {} : { required })}
      error={error}
      {...(hint === undefined ? {} : { hint })}
      render={({ id, describedBy }) => (
        <select
          {...rest}
          id={id}
          className="ui-field__control"
          required={required ?? false}
          aria-invalid={error !== undefined}
          aria-describedby={describedBy}
        >
          {children}
        </select>
      )}
    />
  );
}
