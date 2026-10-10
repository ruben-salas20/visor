# 0014. Arrastrar desde el panel con una capa GTK invisible

**Estado:** Aceptada

## Contexto

El usuario arrastra archivos del panel a una zona de la ventana. Una web no debe ver ni recibir ese arrastre.

## Decisión

El panel avisa al empezar y al terminar el arrastre. Mientras tanto, cada lado muestra una capa GTK invisible que lo recibe y dice qué zona hay bajo el puntero: una mitad o, con la ventana dividida, el lado entero. La marca es un recuadro de cristal esmerilado que dibuja la página (`backdrop-filter`), porque GTK 3 no puede difuminar lo que hay bajo un widget. La página solo recibe la marca; no se lee nada de ella.

Soltar a la izquierda en una ventana sin dividir intercambia los lados tras crear el derecho.
