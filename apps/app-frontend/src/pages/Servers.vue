<script setup lang="ts">
import type { Labrinth } from '@kael/api-client'
import { injectModrinthClient, ServersManagePageIndex } from '@kael/ui'
import { useQuery } from '@tanstack/vue-query'
import { computed } from 'vue'

import { config } from '../config'

const stripePublishableKey = (config.stripePublishableKey as string) || ''

const client = injectModrinthClient()

const { data: products } = useQuery({
	queryKey: ['billing', 'products'],
	queryFn: () => client.labrinth.billing_internal.getProducts(),
})

const resolvedProducts = computed<Labrinth.Billing.Internal.Product[]>(() => products.value ?? [])
</script>

<template>
	<ServersManagePageIndex
		:stripe-publishable-key="stripePublishableKey"
		:products="resolvedProducts"
	/>
</template>
