<script setup lang="ts">
import { computed, ref } from 'vue'

import ButtonStyled from '#ui/components/base/ButtonStyled.vue'
import NewModal from '#ui/components/modal/NewModal.vue'
import SearchSidebarFilter from '#ui/components/search/SearchSidebarFilter.vue'
import { defineMessages, useVIntl } from '#ui/composables/i18n'
import { commonMessages } from '#ui/utils/common-messages'

import { injectBrowseManager } from './providers/browse-manager'

const { formatMessage } = useVIntl()
const ctx = injectBrowseManager()

const modal = ref<InstanceType<typeof NewModal>>()

const visibleFilters = computed(() =>
	ctx.filters.value.filter((filter) => filter.display !== 'none'),
)

const activeTabId = ref<string>('')
const activeFilter = computed(
	() =>
		visibleFilters.value.find((filter) => filter.id === activeTabId.value) ??
		visibleFilters.value[0],
)

function activeCount(filterId: string): number {
	return ctx.currentFilters.value.filter((filter) => filter.type === filterId).length
}

function show() {
	if (!visibleFilters.value.some((filter) => filter.id === activeTabId.value)) {
		activeTabId.value = visibleFilters.value[0]?.id ?? ''
	}
	modal.value?.show()
}

function clearAll() {
	ctx.currentFilters.value = []
}

defineExpose({ show })

const messages = defineMessages({
	header: {
		id: 'browse.filters-modal.header',
		defaultMessage: 'Filters',
	},
	clearAll: {
		id: 'browse.filters-modal.clear-all',
		defaultMessage: 'Clear all',
	},
})
</script>

<template>
	<NewModal ref="modal" :header="formatMessage(messages.header)" width="52rem" max-width="52rem">
		<div class="flex h-[22rem] gap-4">
			<div
				class="flex w-48 shrink-0 flex-col gap-1 overflow-y-auto border-0 border-r border-solid border-surface-5 pr-4"
			>
				<button
					v-for="filter in visibleFilters"
					:key="`filter-tab-${filter.id}`"
					class="flex cursor-pointer items-center justify-between gap-2 rounded-lg border-none px-3 py-2 text-left text-sm font-semibold transition-colors"
					:class="
						activeFilter?.id === filter.id
							? 'bg-button-bg text-contrast'
							: 'bg-transparent text-secondary hover:bg-button-bg hover:text-contrast'
					"
					@click="activeTabId = filter.id"
				>
					<span class="truncate">{{ filter.formatted_name }}</span>
					<span
						v-if="activeCount(filter.id) > 0"
						class="flex h-5 min-w-5 shrink-0 items-center justify-center rounded-full bg-brand px-1.5 text-xs font-bold text-brand-inverted"
					>
						{{ activeCount(filter.id) }}
					</span>
				</button>
			</div>

			<div class="min-w-0 flex-1 overflow-y-auto">
				<SearchSidebarFilter
					v-if="activeFilter"
					:key="`filter-panel-${activeFilter.id}`"
					v-model:selected-filters="ctx.currentFilters.value"
					v-model:toggled-groups="ctx.toggledGroups.value"
					v-model:overridden-provided-filter-types="ctx.overriddenProvidedFilterTypes.value"
					:provided-filters="ctx.providedFilters?.value ?? []"
					:filter-type="activeFilter"
					:force-open="true"
					:hide-exclude="true"
					button-class="!hidden"
				/>
			</div>
		</div>

		<template #actions>
			<div class="flex flex-wrap justify-end gap-2">
				<ButtonStyled type="outlined">
					<button @click="clearAll">
						{{ formatMessage(messages.clearAll) }}
					</button>
				</ButtonStyled>
				<ButtonStyled color="brand">
					<button @click="modal?.hide()">
						{{ formatMessage(commonMessages.doneLabel) }}
					</button>
				</ButtonStyled>
			</div>
		</template>
	</NewModal>
</template>
