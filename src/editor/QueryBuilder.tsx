// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The query builder: a form for a tasks or Dataview query, the query it makes (editable), and its
// results as the block would show them, side by side. Opened from the editor (query help, the
// fence's "Build query…", the toolbar) or the palette; Insert writes the block, or replaces the
// one it was opened on (src/editor/queryBuilderStore.ts). The form logic is src/editor/queryBuild.ts.

import { useEffect, useId, useState } from "react";
import { DatePicker } from "../DatePicker";
import { Icon } from "../icons";
import { DataviewBlock } from "../md/Dataview";
import { QueryDocsLink } from "../md/queryDocs";
import { TaskBlock } from "../md/TaskBlock";
import { scriptsAllowed } from "../md/scripts";
import { place } from "../nav";
import { useStore } from "../store";
import { GROUP_KEYS, LAYOUT_ELEMENTS, PRIORITY_NAMES, SORT_KEYS } from "../tasksq/query";
import { Dialog, Popover, Seg, useDebounced } from "../ui";
import {
  DateOp,
  DvForm,
  DvOp,
  DvType,
  dvText,
  emptyDvForm,
  emptyTasksForm,
  parseDvForm,
  parseTasksForm,
  TaskDateField,
  TasksForm,
  tasksText,
} from "./queryBuild";
import { closeQueryBuilder, queryBuilder, QueryBuilderRequest } from "./queryBuilderStore";
import { vaultVocab, VaultVocab } from "./queryVocab";

const DATE_FIELDS: [TaskDateField, string][] = [
  ["due", "Due"],
  ["scheduled", "Deferred"],
  ["starts", "Starts"],
  ["created", "Created"],
  ["done", "Done"],
  ["happens", "Happens"],
];
const DATE_OPS: DateOp[] = ["before", "after", "on", "in", "on or before", "on or after"];
const DV_OPS: DvOp[] = ["=", "!=", "<", ">", "<=", ">=", "contains", "does not contain", "is true", "is false"];

/** Mounted once in App; shows the builder when one is asked for. */
export function QueryBuilderHost() {
  const r = useStore(queryBuilder);
  return r ? <QueryBuilder key={`${r.lang}:${r.text}`} req={r} /> : null;
}

function useVocab(): VaultVocab {
  const [v, setV] = useState<VaultVocab>({ tags: [], folders: [], notes: [], fields: [] });
  useEffect(() => {
    let live = true;
    vaultVocab()
      .then((x) => live && setV(x))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  return v;
}

export function QueryBuilder({ req }: { req: QueryBuilderRequest }) {
  const editing = !!req.text.trim();
  const [lang, setLang] = useState<"tasks" | "dataview">(req.lang);
  const [tf, setTf] = useState<TasksForm>(() => (req.lang === "tasks" && editing ? parseTasksForm(req.text) : emptyTasksForm()));
  const [df, setDf] = useState<DvForm>(() =>
    req.lang === "dataview" && editing ? parseDvForm(req.text) : emptyDvForm((req.kind as DvType | undefined) ?? "TABLE"),
  );
  // The query as text: made by the form until it's edited by hand.
  const [hand, setHand] = useState<string | null>(null);
  const made = lang === "tasks" ? tasksText(tf) : dvText(df);
  const text = hand ?? made;
  const shown = useDebounced(text, 300);
  const path = useStore(place).place.path ?? "";
  const vocab = useVocab();
  const extra = lang === "tasks" ? tf.extra : df.extra;
  const insert = () => {
    req.apply(text, lang);
    closeQueryBuilder();
  };

  return (
    <Dialog onClose={closeQueryBuilder} width={1040} label="Query builder">
      <div className="qb">
        <div className="qbhead">
          <h2 className="h2 grow">{editing ? "Edit query" : "Build a query"}</h2>
          {!editing && (
            <Seg<"tasks" | "dataview">
              label="Language"
              value={lang}
              onChange={(l) => {
                setLang(l);
                setHand(null);
              }}
              options={[
                ["tasks", "Tasks"],
                ["dataview", "Dataview"],
              ]}
            />
          )}
          <QueryDocsLink lang={lang} />
          <button
            type="button"
            className="ibtn"
            aria-label="Close"
            title="Close without changing the note (Esc)"
            onClick={closeQueryBuilder}
          >
            <Icon name="x" />
          </button>
        </div>
        <div className="qbbody">
          <div className="qbform">
            {hand !== null && (
              <div className="qbnote">
                The query was edited by hand, so the form no longer makes it.
                <button
                  type="button"
                  className="btn ghost sm"
                  title="Drop the hand edits and build the query from the form"
                  onClick={() => setHand(null)}
                >
                  Use the form again
                </button>
              </div>
            )}
            {extra.length > 0 && hand === null && (
              <div className="qbnote">
                The form can't show {extra.length === 1 ? "one line" : `${extra.length} lines`} of this query;{" "}
                {extra.length === 1 ? "it's" : "they're"} kept as written: <code>{extra.join(" · ")}</code>
              </div>
            )}
            {lang === "tasks" ? <TasksFields f={tf} set={setTf} vocab={vocab} /> : <DvFields f={df} set={setDf} vocab={vocab} />}
          </div>
          <div className="qbside">
            <label className="qbtext">
              <span className="eyebrow">Query</span>
              <textarea
                className="mono"
                value={text}
                spellCheck={false}
                rows={8}
                onChange={(e) => setHand(e.target.value)}
                aria-label="Query text"
              />
            </label>
            <div className="qbprev prose">
              <span className="eyebrow">Preview</span>
              {lang === "tasks" ? (
                <TaskBlock src={shown} path={path} scripts={scriptsAllowed(path)} />
              ) : (
                <DataviewBlock lang="dataview" src={shown} path={path} scripts={false} />
              )}
            </div>
          </div>
        </div>
        <div className="qbfoot">
          <span className="faint small">{editing ? "Replaces the block's query." : "Inserts a block at the caret."}</span>
          <span className="sp" />
          <button type="button" className="btn" title="Close without changing the note (Esc)" onClick={closeQueryBuilder}>
            Cancel
          </button>
          <span
            title={
              !text.trim()
                ? "The query is empty"
                : editing
                  ? "Replace the block’s query with this one"
                  : "Insert the query as a block at the caret"
            }
          >
            <button type="button" className="btn pri" onClick={insert} disabled={!text.trim()}>
              {editing ? "Update" : "Insert"}
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}

// ── Pieces ───────────────────────────────────────────────────────────────────────────────────

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="qbrow">
      <span className="qblabel">{label}</span>
      <div className="qbctl">{children}</div>
    </div>
  );
}

function Text({
  value,
  onChange,
  placeholder,
  list,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  list?: string;
  label: string;
}) {
  return (
    <span className="inp sm">
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        list={list}
        aria-label={label}
        spellCheck={false}
      />
    </span>
  );
}

function Select<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: (T | [T, string])[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <select title={label} className="inp sm" value={value} onChange={(e) => onChange(e.target.value as T)} aria-label={label}>
      {options.map((o) => {
        const [v, l] = Array.isArray(o) ? o : [o, o];
        return (
          <option key={v} value={v}>
            {l}
          </option>
        );
      })}
    </select>
  );
}

function Remove({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button type="button" className="ibtn xs" aria-label={label} title={label} onClick={onClick}>
      <Icon name="x" size={12} />
    </button>
  );
}

function Add({ onClick, title, children }: { onClick: () => void; title: string; children: React.ReactNode }) {
  return (
    <button type="button" className="btn ghost sm qbadd" title={title} onClick={onClick}>
      <Icon name="plus" size={13} />
      {children}
    </button>
  );
}

/** A list of short texts (tags, folders) with a picker of the vault's own. */
function Chips({
  values,
  onChange,
  options,
  placeholder,
  label,
}: {
  values: string[];
  onChange: (v: string[]) => void;
  options: string[];
  placeholder: string;
  label: string;
}) {
  const [typed, setTyped] = useState("");
  const id = `qb-${useId().replace(/:/g, "")}`;
  const add = () => {
    const t = typed.trim();
    if (t && !values.includes(t)) onChange([...values, t]);
    setTyped("");
  };
  return (
    <div className="qbchips">
      {values.map((v) => (
        <span key={v} className="chip">
          {v}
          <Remove label={`Remove ${v}`} onClick={() => onChange(values.filter((x) => x !== v))} />
        </span>
      ))}
      <span className="inp sm">
        <input
          value={typed}
          list={id}
          placeholder={placeholder}
          aria-label={label}
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
          onBlur={add}
        />
      </span>
      <datalist id={id}>
        {options.map((o) => (
          <option key={o} value={o} />
        ))}
      </datalist>
    </div>
  );
}

/** A date for a tasks query: a word (today, next week) or a day from the calendar. */
function DateWord({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  return (
    <span className="row qbdate">
      <Text value={value} onChange={onChange} placeholder="today, next week, 2026-10-05" list="qb-datewords" label="Date" />
      <button
        type="button"
        className="ibtn"
        aria-label="Pick a day"
        title="Pick a day from the calendar"
        onClick={(e) => setAt(e.currentTarget.getBoundingClientRect())}
      >
        <Icon name="calendar" size={14} />
      </button>
      <datalist id="qb-datewords">
        {["today", "tomorrow", "yesterday", "this week", "next week", "last week", "this month", "next month", "in 3 days"].map((w) => (
          <option key={w} value={w} />
        ))}
      </datalist>
      {at && (
        <Popover anchor={at} onClose={() => setAt(null)} width={252}>
          <DatePicker
            value={/^\d{4}-\d{2}-\d{2}$/.test(value) ? value : null}
            label="Date"
            onPick={(d) => {
              setAt(null);
              onChange(d);
            }}
            onCancel={() => setAt(null)}
          />
        </Popover>
      )}
    </span>
  );
}

// ── Tasks ────────────────────────────────────────────────────────────────────────────────────

function TasksFields({ f, set, vocab }: { f: TasksForm; set: (f: TasksForm) => void; vocab: VaultVocab }) {
  const up = (p: Partial<TasksForm>) => set({ ...f, ...p });
  return (
    <>
      <Row label="Show">
        <Seg<TasksForm["status"]>
          label="Status"
          value={f.status}
          onChange={(status) => up({ status })}
          options={[
            ["not done", "Not done"],
            ["done", "Done"],
            ["any", "All"],
          ]}
        />
      </Row>
      <Row label="Dates">
        {f.dates.map((d, i) => (
          <div key={i} className="row qbline">
            <Select
              value={d.field}
              options={DATE_FIELDS}
              label="Date"
              onChange={(field) => up({ dates: f.dates.map((x, j) => (j === i ? { ...x, field } : x)) })}
            />
            <Select
              value={d.op}
              options={DATE_OPS}
              label="How"
              onChange={(op) => up({ dates: f.dates.map((x, j) => (j === i ? { ...x, op } : x)) })}
            />
            <DateWord value={d.value} onChange={(value) => up({ dates: f.dates.map((x, j) => (j === i ? { ...x, value } : x)) })} />
            <Remove label="Remove this date" onClick={() => up({ dates: f.dates.filter((_, j) => j !== i) })} />
          </div>
        ))}
        <Add onClick={() => up({ dates: [...f.dates, { field: "due", op: "before", value: "tomorrow" }] })} title="Add a date filter">
          Date
        </Add>
      </Row>
      <Row label="Tags">
        <Chips
          values={f.tagsInclude}
          onChange={(tagsInclude) => up({ tagsInclude })}
          options={vocab.tags}
          placeholder="with #tag"
          label="Tags to include"
        />
        <Chips
          values={f.tagsExclude}
          onChange={(tagsExclude) => up({ tagsExclude })}
          options={vocab.tags}
          placeholder="without #tag"
          label="Tags to leave out"
        />
      </Row>
      <Row label="In">
        <Chips
          values={f.folders}
          onChange={(folders) => up({ folders })}
          options={vocab.folders}
          placeholder="folder or path"
          label="Folders"
        />
      </Row>
      <Row label="Text">
        <Text
          value={f.description}
          onChange={(description) => up({ description })}
          placeholder="task text includes…"
          label="Task text includes"
        />
        <Text value={f.heading} onChange={(heading) => up({ heading })} placeholder="under a heading with…" label="Heading includes" />
      </Row>
      <Row label="Priority">
        <Select<"any" | "is" | "is above" | "is below">
          value={f.priority?.op ?? "any"}
          options={[["any", "Any"], "is", "is above", "is below"]}
          label="Priority"
          onChange={(op) => up({ priority: op === "any" ? null : { op, value: f.priority?.value ?? "high" } })}
        />
        {f.priority && (
          <Select
            value={f.priority.value}
            options={[...PRIORITY_NAMES]}
            label="Priority level"
            onChange={(value) => up({ priority: { ...f.priority!, value } })}
          />
        )}
        <Seg<TasksForm["recurring"]>
          label="Repeating"
          value={f.recurring}
          onChange={(recurring) => up({ recurring })}
          options={[
            ["any", "Any"],
            ["yes", "Repeating"],
            ["no", "One-off"],
          ]}
        />
      </Row>
      <Row label="Sort">
        {f.sort.map((s, i) => (
          <div key={i} className="row qbline">
            <Select
              value={s.key}
              options={SORT_KEYS}
              label="Sort by"
              onChange={(key) => up({ sort: f.sort.map((x, j) => (j === i ? { ...x, key } : x)) })}
            />
            <label className="qbcheck">
              <input
                type="checkbox"
                checked={s.reverse}
                onChange={(e) => up({ sort: f.sort.map((x, j) => (j === i ? { ...x, reverse: e.target.checked } : x)) })}
              />{" "}
              reverse
            </label>
            <Remove label="Remove this sort" onClick={() => up({ sort: f.sort.filter((_, j) => j !== i) })} />
          </div>
        ))}
        <Add onClick={() => up({ sort: [...f.sort, { key: "due", reverse: false }] })} title="Add another sort key">
          Sort
        </Add>
      </Row>
      <Row label="Group">
        {f.group.map((g, i) => (
          <div key={i} className="row qbline">
            <Select
              value={g.key}
              options={GROUP_KEYS}
              label="Group by"
              onChange={(key) => up({ group: f.group.map((x, j) => (j === i ? { ...x, key } : x)) })}
            />
            <label className="qbcheck">
              <input
                type="checkbox"
                checked={g.reverse}
                onChange={(e) => up({ group: f.group.map((x, j) => (j === i ? { ...x, reverse: e.target.checked } : x)) })}
              />{" "}
              reverse
            </label>
            <Remove label="Remove this group" onClick={() => up({ group: f.group.filter((_, j) => j !== i) })} />
          </div>
        ))}
        <Add onClick={() => up({ group: [...f.group, { key: "filename", reverse: false }] })} title="Add another grouping">
          Group
        </Add>
      </Row>
      <Row label="Layout">
        <span className="inp sm qbnum">
          <input
            type="number"
            min={1}
            value={f.limit ?? ""}
            placeholder="no limit"
            aria-label="Limit"
            onChange={(e) => up({ limit: e.target.value ? Math.max(1, +e.target.value) : null })}
          />
        </span>
        <label className="qbcheck">
          <input type="checkbox" checked={f.shortMode} onChange={(e) => up({ shortMode: e.target.checked })} /> short mode
        </label>
        <label className="qbcheck">
          <input type="checkbox" checked={f.explain} onChange={(e) => up({ explain: e.target.checked })} /> explain
        </label>
        <Chips values={f.hide} onChange={(hide) => up({ hide })} options={[...LAYOUT_ELEMENTS]} placeholder="hide…" label="Hide" />
      </Row>
    </>
  );
}

// ── Dataview ─────────────────────────────────────────────────────────────────────────────────

const FILE_FIELDS = [
  "file.name",
  "file.link",
  "file.folder",
  "file.path",
  "file.ctime",
  "file.cday",
  "file.mtime",
  "file.mday",
  "file.tags",
  "file.size",
  "file.day",
];

function DvFields({ f, set, vocab }: { f: DvForm; set: (f: DvForm) => void; vocab: VaultVocab }) {
  const up = (p: Partial<DvForm>) => set({ ...f, ...p });
  const fields = [...FILE_FIELDS, ...vocab.fields];
  return (
    <>
      <datalist id="qb-fields">
        {fields.map((x) => (
          <option key={x} value={x} />
        ))}
      </datalist>
      <Row label="Show as">
        <Seg<DvType>
          label="Type"
          value={f.type}
          onChange={(type) => up({ type, fields: type === f.type ? f.fields : emptyDvForm(type).fields })}
          options={[
            ["TABLE", "Table"],
            ["LIST", "List"],
            ["TASK", "Tasks"],
            ["CALENDAR", "Calendar"],
          ]}
        />
        {(f.type === "TABLE" || f.type === "LIST") && (
          <label className="qbcheck">
            <input type="checkbox" checked={f.withoutId} onChange={(e) => up({ withoutId: e.target.checked })} /> without the note column
          </label>
        )}
      </Row>
      {f.type === "TABLE" && (
        <Row label="Columns">
          {f.fields.map((c, i) => (
            <div key={i} className="row qbline">
              <Text
                value={c.expr}
                onChange={(expr) => up({ fields: f.fields.map((x, j) => (j === i ? { ...x, expr } : x)) })}
                list="qb-fields"
                placeholder="field"
                label="Column field"
              />
              <Text
                value={c.alias}
                onChange={(alias) => up({ fields: f.fields.map((x, j) => (j === i ? { ...x, alias } : x)) })}
                placeholder="heading"
                label="Column heading"
              />
              <Remove label="Remove this column" onClick={() => up({ fields: f.fields.filter((_, j) => j !== i) })} />
            </div>
          ))}
          <Add onClick={() => up({ fields: [...f.fields, { expr: "", alias: "" }] })} title="Add a column to the table">
            Column
          </Add>
        </Row>
      )}
      {(f.type === "LIST" || f.type === "CALENDAR") && (
        <Row label={f.type === "LIST" ? "Show with" : "Date"}>
          <Text
            value={f.fields[0]?.expr ?? ""}
            onChange={(expr) => up({ fields: [{ expr, alias: "" }] })}
            list="qb-fields"
            placeholder={f.type === "LIST" ? "a field beside each (optional)" : "file.day"}
            label={f.type === "LIST" ? "Value" : "Date field"}
          />
        </Row>
      )}
      <Row label="From">
        {f.from.map((s, i) => (
          <div key={i} className="row qbline">
            <Select
              value={s.kind}
              options={[
                ["tag", "Tag"],
                ["folder", "Folder"],
                ["note", "Linking to"],
                ["outgoing", "Linked from"],
              ]}
              label="Source"
              onChange={(kind) => up({ from: f.from.map((x, j) => (j === i ? { ...x, kind } : x)) })}
            />
            <Text
              value={s.value}
              onChange={(value) => up({ from: f.from.map((x, j) => (j === i ? { ...x, value } : x)) })}
              list={s.kind === "tag" ? "qb-tags" : s.kind === "folder" ? "qb-folders" : "qb-notes"}
              placeholder={s.kind === "tag" ? "tag" : s.kind === "folder" ? "folder" : "note"}
              label="Source value"
            />
            <label className="qbcheck">
              <input
                type="checkbox"
                checked={s.not}
                onChange={(e) => up({ from: f.from.map((x, j) => (j === i ? { ...x, not: e.target.checked } : x)) })}
              />{" "}
              leave out
            </label>
            <Remove label="Remove this source" onClick={() => up({ from: f.from.filter((_, j) => j !== i) })} />
          </div>
        ))}
        <div className="row">
          <Add onClick={() => up({ from: [...f.from, { kind: "tag", value: "", not: false }] })} title="Add a source the pages come from">
            Source
          </Add>
          {f.from.length > 1 && (
            <Seg<"and" | "or">
              label="Sources joined by"
              value={f.fromJoin}
              onChange={(fromJoin) => up({ fromJoin })}
              options={[
                ["and", "All of them"],
                ["or", "Any of them"],
              ]}
            />
          )}
        </div>
        <datalist id="qb-tags">
          {vocab.tags.map((t) => (
            <option key={t} value={t.replace(/^#/, "")} />
          ))}
        </datalist>
        <datalist id="qb-folders">
          {vocab.folders.map((t) => (
            <option key={t} value={t} />
          ))}
        </datalist>
        <datalist id="qb-notes">
          {vocab.notes.map((t) => (
            <option key={t} value={t} />
          ))}
        </datalist>
      </Row>
      <Row label="Where">
        {f.where.map((w, i) => (
          <div key={i} className="row qbline">
            <Text
              value={w.field}
              onChange={(field) => up({ where: f.where.map((x, j) => (j === i ? { ...x, field } : x)) })}
              list="qb-fields"
              placeholder="field"
              label="Condition field"
            />
            <Select
              value={w.op}
              options={DV_OPS}
              label="Operator"
              onChange={(op) => up({ where: f.where.map((x, j) => (j === i ? { ...x, op } : x)) })}
            />
            {w.op !== "is true" && w.op !== "is false" && (
              <Text
                value={w.value}
                onChange={(value) => up({ where: f.where.map((x, j) => (j === i ? { ...x, value } : x)) })}
                placeholder="value, date(today)…"
                label="Condition value"
              />
            )}
            <Remove label="Remove this condition" onClick={() => up({ where: f.where.filter((_, j) => j !== i) })} />
          </div>
        ))}
        <div className="row">
          <Add onClick={() => up({ where: [...f.where, { field: "", op: "=", value: "" }] })} title="Add a condition the rows must meet">
            Condition
          </Add>
          {f.where.length > 1 && (
            <Seg<"AND" | "OR">
              label="Conditions joined by"
              value={f.whereJoin}
              onChange={(whereJoin) => up({ whereJoin })}
              options={[
                ["AND", "All of them"],
                ["OR", "Any of them"],
              ]}
            />
          )}
        </div>
      </Row>
      <Row label="Sort">
        {f.sort.map((s, i) => (
          <div key={i} className="row qbline">
            <Text
              value={s.field}
              onChange={(field) => up({ sort: f.sort.map((x, j) => (j === i ? { ...x, field } : x)) })}
              list="qb-fields"
              placeholder="field"
              label="Sort field"
            />
            <Select
              value={s.dir}
              options={[
                ["ASC", "Ascending"],
                ["DESC", "Descending"],
              ]}
              label="Direction"
              onChange={(dir) => up({ sort: f.sort.map((x, j) => (j === i ? { ...x, dir } : x)) })}
            />
            <Remove label="Remove this sort" onClick={() => up({ sort: f.sort.filter((_, j) => j !== i) })} />
          </div>
        ))}
        <Add onClick={() => up({ sort: [...f.sort, { field: "file.mtime", dir: "DESC" }] })} title="Add another sort key">
          Sort
        </Add>
      </Row>
      <Row label="More">
        <Text value={f.groupBy} onChange={(groupBy) => up({ groupBy })} list="qb-fields" placeholder="group by field" label="Group by" />
        <Text value={f.flatten} onChange={(flatten) => up({ flatten })} list="qb-fields" placeholder="flatten list field" label="Flatten" />
        <span className="inp sm qbnum">
          <input
            type="number"
            min={1}
            value={f.limit ?? ""}
            placeholder="no limit"
            aria-label="Limit"
            onChange={(e) => up({ limit: e.target.value ? Math.max(1, +e.target.value) : null })}
          />
        </span>
      </Row>
    </>
  );
}
