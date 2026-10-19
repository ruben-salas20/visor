<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { cn } from '$lib/utils';
	import { activar, cerrar, nuevaPestana, ui, type Pestana } from '$lib/marco.svelte';
	import { Plus, X } from '@lucide/svelte';

	let { lado, pestanas }: { lado: number; pestanas: Pestana[] } = $props();

	const nombreDe = (ruta: string) => ruta.split('/').pop() ?? ruta;
</script>

<div
	role="group"
	class={cn('panel flex min-w-0 flex-1 flex-col bg-background', ui.foco === lado && 'foco')}
	data-lado={lado}
>
	<div
		class={cn(
			'pestanas flex h-10 shrink-0 items-end gap-1 overflow-hidden border-b bg-muted/50 px-2 pt-1.5',
			ui.foco === lado && 'shadow-[inset_0_-2px_0_var(--focus)]',
		)}
	>
		{#each pestanas as t (t.id)}
			<div
				role="tab"
				tabindex="0"
				aria-selected={t.activa}
				title={t.ruta}
				data-lado={lado}
				data-id={t.id}
				class={cn(
					'pestana group flex h-8 min-w-0 max-w-56 cursor-pointer items-center gap-2 rounded-t-lg border border-b-0 px-3 text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring',
					t.activa
						? 'activa border-border bg-card font-medium text-foreground shadow-[0_1px_0_var(--card)]'
						: 'border-transparent text-muted-foreground hover:bg-background/70 hover:text-foreground',
				)}
				onclick={() => activar(lado, t.id)}
				onkeydown={(e) => {
					if (e.key === 'Enter' || e.key === ' ') {
						e.preventDefault();
						activar(lado, t.id);
					}
				}}
			>
				<span class="truncate">{nombreDe(t.ruta)}</span>
				<button
					type="button"
					class="cerrar -mr-1 grid size-4 shrink-0 place-items-center rounded-sm text-muted-foreground opacity-60 transition-opacity hover:bg-muted hover:opacity-100"
					data-lado={lado}
					data-id={t.id}
					aria-label="Cerrar pestaña"
					onclick={(e) => {
						// Without this, closing a tab would also activate it.
						e.stopPropagation();
						cerrar(lado, t.id);
					}}
				>
					<X class="size-3" />
				</button>
			</div>
		{/each}
		<Button
			variant="ghost"
			size="sm"
			class="nueva mb-1 h-7 shrink-0 text-muted-foreground"
			data-lado={lado}
			onclick={() => nuevaPestana(lado)}
		>
			<Plus class="size-3.5" />
			Nueva pestaña
		</Button>
	</div>
	<div class="contenido min-h-0 flex-1 bg-background" data-lado={lado}></div>
</div>
