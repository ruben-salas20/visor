# Diagramas

Cambia entre un tema claro y uno oscuro de Omarchy y comprueba que estos diagramas y fórmulas se leen bien en los dos.

## Diagrama de flujo

```mermaid
flowchart LR
  A[Pregunta] --> B{¿La entiendo?}
  B -- Sí --> C[Siguiente tema]
  B -- No --> D[Pedir explicación al agente] --> A
```

## Mapa mental

```mermaid
mindmap
  root((Ondas))
    Mecánicas
      Sonido
      Sísmicas
    Electromagnéticas
      Luz
      Radio
    Magnitudes
      Frecuencia
      Amplitud
```

## Secuencia

```mermaid
sequenceDiagram
  Persona->>Agente: Explícame la difracción
  Agente->>visor: Crea difraccion.html
  visor-->>Persona: Muestra la página
```

## Fórmulas

La energía de un fotón es $E = h f$, y la relación de dispersión:

$$
\omega^2 = c^2 k^2 + \frac{m^2 c^4}{\hbar^2}
$$

## Código

```rust
fn frecuencia(periodo: f64) -> f64 {
    1.0 / periodo
}
```
