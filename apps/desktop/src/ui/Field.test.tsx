// Tests for Field.tsx — InputField, SelectField, TextareaField with label/hint/error.

import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test } from 'vitest';
import { InputField, SelectField, TextareaField } from './Field';

afterEach(() => cleanup());

// ─── InputField ─────────────────────────────────────────────────────────────

describe('InputField', () => {
  test('renders input with generated id', () => {
    const { container } = render(<InputField />);
    const input = container.querySelector('input')!;
    expect(input).toBeInTheDocument();
    expect(input.id).toBeTruthy();
  });

  test('label is rendered and associated via htmlFor', () => {
    render(<InputField id="name-field" label="Name" />);
    const label = screen.getByText('Name');
    expect(label).toHaveAttribute('for', 'name-field');
  });

  test('hint is rendered when no error', () => {
    render(<InputField hint="This is a hint" />);
    expect(screen.getByText('This is a hint')).toBeInTheDocument();
  });

  test('error message hides hint', () => {
    render(<InputField hint="a hint" error="Required" />);
    expect(screen.queryByText('a hint')).not.toBeInTheDocument();
    expect(screen.getByText('Required')).toBeInTheDocument();
  });

  test('vertical layout (default) uses flex-column direction', () => {
    const { container } = render(<InputField label="L" />);
    const field = container.querySelector('.field') as HTMLElement;
    expect(field.style.flexDirection).toBe('column');
  });

  test('input has input--box class', () => {
    const { container } = render(<InputField />);
    const input = container.querySelector('input')!;
    expect(input.className).toContain('input--box');
  });

  test('passes extra props to input (placeholder, type)', () => {
    render(<InputField placeholder="Type here" type="email" />);
    const input = screen.getByPlaceholderText('Type here');
    expect(input).toHaveAttribute('type', 'email');
  });

  test('custom id is used on input and label', () => {
    render(<InputField id="custom-id" label="Label" />);
    expect(screen.getByLabelText('Label')).toHaveAttribute('id', 'custom-id');
  });
});

// ─── SelectField ────────────────────────────────────────────────────────────

describe('SelectField', () => {
  test('renders native select', () => {
    const { container } = render(
      <SelectField>
        <option value="a">A</option>
      </SelectField>,
    );
    expect(container.querySelector('select')).toBeInTheDocument();
  });

  test('label associates with select via htmlFor', () => {
    render(
      <SelectField id="sel" label="Choose">
        <option value="x">X</option>
      </SelectField>,
    );
    expect(screen.getByLabelText('Choose')).toBeInTheDocument();
  });

  test('error is shown when provided', () => {
    render(
      <SelectField error="Pick one">
        <option>opt</option>
      </SelectField>,
    );
    expect(screen.getByText('Pick one')).toBeInTheDocument();
  });

  test('hint renders when no error', () => {
    render(
      <SelectField hint="helpful">
        <option>opt</option>
      </SelectField>,
    );
    expect(screen.getByText('helpful')).toBeInTheDocument();
  });
});

// ─── TextareaField ──────────────────────────────────────────────────────────

describe('TextareaField', () => {
  test('renders textarea element', () => {
    const { container } = render(<TextareaField />);
    expect(container.querySelector('textarea')).toBeInTheDocument();
  });

  test('label associates with textarea', () => {
    render(<TextareaField id="ta" label="Notes" />);
    expect(screen.getByLabelText('Notes')).toBeInTheDocument();
  });

  test('textarea has input--box class', () => {
    const { container } = render(<TextareaField />);
    const ta = container.querySelector('textarea')!;
    expect(ta.className).toContain('input--box');
  });

  test('error shown when provided', () => {
    render(<TextareaField error="Too long" />);
    expect(screen.getByText('Too long')).toBeInTheDocument();
  });

  test('hint shown when no error', () => {
    render(<TextareaField hint="Max 500 chars" />);
    expect(screen.getByText('Max 500 chars')).toBeInTheDocument();
  });
});
