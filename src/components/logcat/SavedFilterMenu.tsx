import { For, Show, createSignal } from "solid-js";
import { Button, Icon, MenuList, MenuSectionHeader, Popover, Separator } from "@/components/ui";
import { MAX_SAVED_FILTERS, type SavedFilter } from "@/lib/logcat-filter-storage";
import { BUILTIN_LOGCAT_FILTER_PRESETS, commitSavedFilterQuery } from "./saved-filter-presets";
import {
  SavedFilterActionButton,
  SavedFilterApplyItem,
  SavedFilterCountHeader,
  SavedFilterEmptyState,
  SavedFilterItemLayout,
  SavedFilterMenuRow,
  SavedFilterQueryPreview,
  SavedFilterRenameInput,
} from "./SavedFilterMenuParts";

export function SavedFilterMenu(props: {
  savedFilters: readonly SavedFilter[];
  onApplyQuery: (query: string) => void;
  onDeleteSavedFilter: (id: string) => void;
  onRenameSavedFilter: (id: string, name: string) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const [renamingId, setRenamingId] = createSignal<string | null>(null);
  const [renameDraft, setRenameDraft] = createSignal("");

  function applyPreset(q: string) {
    props.onApplyQuery(commitSavedFilterQuery(q));
    setOpen(false);
  }

  function startRename(filter: SavedFilter) {
    setRenamingId(filter.id);
    setRenameDraft(filter.name);
  }

  function commitRename() {
    const id = renamingId();
    if (id) {
      props.onRenameSavedFilter(id, renameDraft());
    }
    setRenamingId(null);
    setRenameDraft("");
  }

  function cancelRename() {
    setRenamingId(null);
    setRenameDraft("");
  }

  function closeMenu() {
    setOpen(false);
    cancelRename();
  }

  return (
    <Popover
      open={open()}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          setRenamingId(null);
          setOpen(true);
        } else {
          closeMenu();
        }
      }}
      align="left"
      minWidth="260px"
      maxWidth="340px"
      trigger={() => (
        <Button
          variant="outline"
          size="xs"
          tone="muted"
          title="Filter presets"
          onClick={() => {
            setRenamingId(null);
            setOpen((v) => !v);
          }}
        >
          <Icon name="list" size={12} /> Filters
        </Button>
      )}
    >
      <Show when={open()}>
        <>
          <MenuList style={{ padding: "6px 0" }}>
            <MenuSectionHeader label="Quick Filters" />
            <For each={BUILTIN_LOGCAT_FILTER_PRESETS}>
              {(p) => (
                <SavedFilterMenuRow onClick={() => applyPreset(p.query)} mono>
                  <span style={{ flex: "1" }}>{p.name}</span>
                  <SavedFilterQueryPreview query={p.query} />
                </SavedFilterMenuRow>
              )}
            </For>

            <Separator spacing="sm" />
            <SavedFilterCountHeader count={props.savedFilters.length} max={MAX_SAVED_FILTERS} />

            <Show when={props.savedFilters.length === 0}>
              <SavedFilterEmptyState />
            </Show>

            <For each={props.savedFilters}>
              {(f) => (
                <Show
                  when={renamingId() === f.id}
                  fallback={
                    <SavedFilterItemLayout>
                      <SavedFilterApplyItem
                        name={f.name}
                        query={f.query}
                        onApply={() => applyPreset(f.query)}
                      />
                      <SavedFilterActionButton
                        title="Rename"
                        icon="pencil"
                        onClick={() => startRename(f)}
                      />
                      <SavedFilterActionButton
                        title="Delete"
                        icon="trash"
                        onClick={() => props.onDeleteSavedFilter(f.id)}
                      />
                    </SavedFilterItemLayout>
                  }
                >
                  <SavedFilterMenuRow gap="4px">
                    <SavedFilterRenameInput
                      value={renameDraft()}
                      onInput={setRenameDraft}
                      onCommit={commitRename}
                      onCancel={cancelRename}
                    />
                  </SavedFilterMenuRow>
                </Show>
              )}
            </For>
          </MenuList>
        </>
      </Show>
    </Popover>
  );
}
