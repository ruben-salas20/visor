<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { Separator } from '$lib/components/ui/separator';
	import * as Tooltip from '$lib/components/ui/tooltip';
	import Arbol from '$lib/components/marco/Arbol.svelte';
	import Panel from '$lib/components/marco/Panel.svelte';
	import { estado } from '$lib/marco.svelte';
	import { ArrowLeft, ArrowRight, RotateCw } from '@lucide/svelte';

	// Decorative only: this prototype has no history or reload.
	const navegacion = [
		{ Icono: ArrowLeft, texto: 'Atrás' },
		{ Icono: ArrowRight, texto: 'Adelante' },
		{ Icono: RotateCw, texto: 'Recargar' },
	];
</script>

<Tooltip.Provider>
	<div class="flex h-full flex-col">
		<header class="flex h-11 shrink-0 items-center gap-1 border-b bg-card px-3">
			{#each navegacion as { Icono, texto } (texto)}
				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button {...props} variant="ghost" size="icon-sm" aria-label={texto} class="text-muted-foreground">
								<Icono class="size-4" />
							</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content side="bottom">{texto}</Tooltip.Content>
				</Tooltip.Root>
			{/each}
			<Separator orientation="vertical" class="mx-2 h-5" />
			<h1 class="text-sm font-semibold tracking-tight">U1 marco HTML</h1>
			<p class="ml-auto text-xs text-muted-foreground">Ctrl+W cierra la pestaña activa del panel con foco</p>
		</header>
		<div class="flex min-h-0 flex-1">
			<Arbol />
			<main id="paneles" class="flex min-w-0 flex-1 divide-x divide-border">
				{#each estado.paneles as panel, i (i)}
					<Panel lado={i} pestanas={panel.pestanas} />
				{/each}
			</main>
		</div>
	</div>
</Tooltip.Provider>
