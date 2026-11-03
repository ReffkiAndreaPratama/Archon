import { Eye, EyeOff, LockKeyhole } from "lucide-react";
import { useState } from "react";

import { Modal } from "./Modal";

interface PasswordDialogProps {
  title: string;
  message: string;
  confirmLabel: string;
  invalid?: boolean;
  onSubmit: (password: string) => void;
  onCancel: () => void;
}

export function PasswordDialog({
  title,
  message,
  confirmLabel,
  invalid = false,
  onSubmit,
  onCancel,
}: PasswordDialogProps) {
  const [password, setPassword] = useState("");
  const [visible, setVisible] = useState(false);

  const submit = () => {
    if (!password) return;
    onSubmit(password);
  };

  return (
    <Modal
      title={title}
      description={message}
      icon={<LockKeyhole size={19} />}
      onClose={onCancel}
      width="small"
      footer={
        <>
          <button type="button" className="btn btn-ghost" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={submit}
            disabled={!password}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <div className="field">
        <label htmlFor="archive-password">Password</label>
        <div className="input-with-action">
          <input
            id="archive-password"
            autoFocus
            type={visible ? "text" : "password"}
            value={password}
            spellCheck={false}
            autoComplete="off"
            placeholder="Enter the archive password"
            onChange={(event) => setPassword(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") submit();
            }}
          />
          <button
            type="button"
            className="input-action"
            onClick={() => setVisible((value) => !value)}
            aria-label={visible ? "Hide password" : "Show password"}
            title={visible ? "Hide password" : "Show password"}
          >
            {visible ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
        </div>
        {invalid ? (
          <p className="field-error">That password could not decrypt the archive. Try again.</p>
        ) : (
          <p className="field-hint">The password stays in memory only and is never logged.</p>
        )}
      </div>
    </Modal>
  );
}
