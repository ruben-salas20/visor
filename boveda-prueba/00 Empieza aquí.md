# Empieza aquí

Esta bóveda sirve para probar lo que no se puede comprobar de forma automática. Ve marcando cada casilla con un clic a medida que pruebes.

Para abrirla: `cargo run --release` en la carpeta del proyecto y luego **Ctrl+O** para elegir esta carpeta. Eso ya prueba el diálogo de carpetas.

## 1. Notas

- [x] **Botón «+»** (arriba, junto al nombre de la bóveda): crea una nota llamada `mi nota`. Debe abrirse en el editor con el cursor al final.
- [ ] **Enlace a una nota que no existe**: haz clic en [[Nota nueva desde enlace]]. Debe crearla y abrirla en el editor.
- [ ] **Posición al volver de editar**: abre [[edicion]], baja hasta la mitad, pulsa Ctrl+E y otra vez Ctrl+E. Debe quedar donde estabas.
- [ ] **Diagramas con tema oscuro**: abre [[diagramas]], cambia a un tema oscuro de Omarchy y mira si los diagramas se leen bien. Recarga con Ctrl+R si no cambian solos.

## 2. Navegación

- [ ] **Enlaces externos**: abre [[enlaces]] y prueba cada enlace.
- [ ] **Botones laterales del ratón**: tras abrir una web desde [[enlaces]], usa el botón «atrás» del ratón para volver.
- [ ] **Ancho del panel**: arrastra el divisor entre el panel y la página, cierra visor y ábrelo otra vez. Debe conservar el ancho.

## 3. Multimedia y rendimiento

- [ ] **Vídeo de la bóveda**: abre [[video]], reprodúcelo, salta a otro punto y pon pantalla completa (Escape para salir).
- [ ] **YouTube en pantalla completa**: escribe `youtube.com` en la barra de direcciones (Ctrl+L), abre un vídeo y ponlo en pantalla completa.
- [ ] **Three.js**: abre [[threejs]]. Debe ir fluido; el contador de la esquina muestra los fps.

## 4. Agentes

- [ ] **Respuestas de un test**: en una terminal ejecuta `visor events --wait 300`, abre [[test]] y responde. La terminal debe mostrar tu respuesta.

## 5. Anotaciones

Abre [[anotaciones]] para estas pruebas.

- [ ] **Resaltar**: selecciona una frase con el ratón. Debe aparecer una barrita con 4 colores y «Comentar». Elige un color.
- [ ] **Comentar texto**: selecciona otra frase, pulsa «Comentar», escribe y guarda (botón o Ctrl+Enter). Debe aparecer un punto de color al final de la frase.
- [ ] **Abrir una anotación**: haz clic sobre un texto resaltado. Debe abrirse su nota; cambia el color y guarda.
- [ ] **Anotar un punto de un gráfico**: pulsa Ctrl+Shift+A (o el botón de diana de la barra), pasa el ratón por la página (se marca el elemento de debajo) y haz clic en una cresta del gráfico. Escribe una nota.
- [ ] **Anotar una imagen y una tabla**: igual que el anterior, sobre la imagen y sobre la tabla.
- [ ] **Lista**: pulsa Ctrl+Shift+L (o el botón de lista). Deben salir todas; al hacer clic en una, la página va hasta ella y la abre.
- [ ] **Respuesta de un agente**: deja abierta en visor la página [[anotaciones]], porque `visor notes` sin más lista solo las anotaciones de la página abierta. En una terminal ejecuta `visor notes`, copia un `id` y ejecuta `visor notes reply <id> Hola desde la terminal`. La respuesta debe aparecer en esa nota. Si no sale ninguna, prueba `visor notes --all`, que lista las de toda la bóveda.
- [ ] **Huérfanas**: edita `pruebas/anotaciones.html` y borra una frase que hayas resaltado. Al guardar, esa anotación debe salir en la lista como «Huérfana».
- [ ] **Eliminar**: abre una anotación y pulsa «Eliminar».
- [ ] **Al cambiar el tamaño de la ventana**, los marcadores deben seguir en su sitio.

## 6. Archivos y ventana

Usa la carpeta `estudio` o crea notas de prueba con el botón «+»: estas pruebas mueven y borran archivos.

- [x] **Papelera con clic derecho**: clic derecho sobre una nota de prueba y «Mover a la papelera». Debe desaparecer del panel, salir un aviso abajo y estar en la papelera del sistema (con su ruta original, para poder restaurarla).
- [x] **Papelera con Supr**: haz clic en una nota de prueba del panel y pulsa Supr.
- [ ] **Carpeta a la papelera**: haz lo mismo con una carpeta de prueba. Si tenía anotaciones, también van a la papelera.
- [ ] **Mover a otra carpeta**: arrastra una nota sobre una carpeta. Al pasar por encima, la carpeta se resalta y, si está cerrada, se abre al cabo de un momento. Si la nota estaba abierta, debe seguir abierta en su nueva ruta y conservar sus anotaciones.
- [x] **Enlaces al mover**: en una nota de prueba escribe `[[estudio/…]]` con la ruta de otra nota y un enlace `[texto](ruta/relativa.md)` hacia ella. Mueve esa otra nota a otra carpeta: debe salir «Se actualizaron los enlaces de 1 nota» y los dos enlaces deben seguir funcionando.
- [x] **Mover a la raíz**: arrastra una nota al hueco vacío de debajo del árbol.
- [x] **Mover una carpeta** dentro de otra. Arrastrarla dentro de sí misma debe mostrar un aviso, no hacer nada raro.
- [ ] **Nombre repetido**: mueve una nota a una carpeta que ya tiene otra con el mismo nombre. Debe salir un aviso y no sobrescribir nada.
- [x] **Nueva carpeta**: pulsa el botón de carpeta (junto al «+»). Debe pedir el nombre y crearla en la carpeta de la página abierta; aparece aunque esté vacía.
- [x] **Carpeta o nota dentro de otra**: clic derecho sobre una carpeta → «Nueva carpeta aquí» y «Nueva nota aquí».
- [x] **Soltar en una carpeta vacía**: arrastra una nota a la carpeta que acabas de crear.
- [x] **Ocultar la barra**: Ctrl+Shift+B la oculta; Ctrl+L la muestra mientras escribes y, al pulsar Escape o Enter, se vuelve a ocultar. Cierra y abre visor: debe seguir oculta. Ctrl+Shift+B la vuelve a mostrar.
- [x] **Tamaño de la página**: Ctrl++ y Ctrl+- cambian el tamaño; Ctrl+0 lo restablece. Prueba en una nota y en una web. Cierra y abre visor: debe conservar el tamaño.

## 7. Anotaciones: diseño y títulos

Sigue en [[anotaciones]].

- [x] **Cita larga**: selecciona casi todo el primer párrafo y pulsa «Comentar». En la cabecera, la cita debe empezar por el principio de tu selección y acabar en «…» antes de los círculos de color, sin montarse sobre ellos.
- [x] **Ventana legible**: con la ventana de la nota abierta sobre texto de la página, no debe leerse nada de la página a través de ella. «Eliminar», «Cancelar» y «Guardar» deben verse como botones, dentro de la ventana.
- [x] **Tema oscuro**: cambia a un tema oscuro de Omarchy y repite las dos pruebas anteriores.
- [x] **Ventana estrecha**: haz la ventana de visor lo más estrecha que puedas y abre una anotación. Todo debe caber; si no hay sitio, los colores pasan a la línea de abajo.
- [x] **Título calculado**: con Ctrl+Shift+A anota el gráfico, la imagen, la tabla y la figura de «Figura con pie». Los títulos deben ser «Diagrama en “Gráfico”», «Espectro», «Tabla en “Tabla”» y «Crestas y valles de una onda». En ningún sitio debe verse un nombre como `<canvas>`.
- [x] **Cambiar el título**: abre una de esas anotaciones, haz clic en su título, escribe otro y guarda. Debe salir con el nuevo nombre en la lista (Ctrl+Shift+L) y en `visor notes` (campo `"title"`).
- [x] **Título vacío**: borra el título entero y guarda. Debe volver el título calculado.
- [x] **Marcador centrado**: el bocadillo de los marcadores redondos (sobre el gráfico o la imagen) debe quedar en el centro del círculo.
- [x] **Anotaciones antiguas**: las que creaste antes de este cambio sobre un elemento deben salir en la lista con un título claro, como «Diagrama en “Gráfico”», y abrirse sin errores.

## 8. Notas: casillas y edición

- [x] **Marcar tareas sin editar**: abre [[tareas]] y haz clic en varias casillas, también en las subtareas, la de la lista numerada y la de la cita. Pulsa Ctrl+E: el texto debe mostrar `[x]` justo en las que marcaste. La página no debe parpadear ni recargarse al hacer clic.
- [x] **Casilla centrada**: la marca ✓ de las casillas marcadas debe quedar en el centro, con tema claro y oscuro.
- [x] **Casilla de un bloque de código**: en [[tareas]], el `- [ ]` del bloque de código no debe ser una casilla.
- [x] **Clics rápidos**: en [[tareas]], marca y desmarca la misma casilla muy deprisa varias veces. Al final, el texto (Ctrl+E) debe coincidir con lo que ves.
- [x] **Abrir el editor donde lees**: abre [[edicion]], baja hasta la sección 10 y pulsa Ctrl+E. El editor debe mostrar la sección 10 a la misma altura, con el cursor en la primera línea visible, no al principio.
- [x] **Volver a la vista donde editas**: en el editor de [[edicion]], baja hasta la sección 14, escribe algo en su párrafo y pulsa Ctrl+E. La vista debe quedar con ese párrafo a la misma altura.
- [x] **Con bloques largos**: abre [[diagramas]], deja un diagrama cortado por el borde de arriba y pulsa Ctrl+E dos veces. Debe volver a la misma zona.

## 9. Editor con vista previa

Abre [[vista-previa]] y pulsa Ctrl+E. La nota debe verse casi igual que al leerla, en la misma posición.

- [x] **Marcas que aparecen y desaparecen**: haz clic en una línea con negrita, cursiva, código o un título. Al entrar en la línea aparecen los `**`, `*`, `` ` `` y `#`; al salir de ella se ocultan.
- [x] **Escribir con acentos**: escribe «canción, pingüino, ¿qué?» con el teclado (con las teclas de tilde, no copiando). Debe salir igual, sin letras repetidas ni el cursor saltando.
- [x] **Deshacer y rehacer**: escribe algo, pulsa Ctrl+Z varias veces y después Ctrl+Mayús+Z. El texto debe ir y volver.
- [x] **Pegar sin formato**: copia un trozo de una web con negritas y enlaces y pégalo. Debe entrar solo el texto.
- [x] **Casillas**: haz clic en las casillas de «Casillas». Se marcan y desmarcan; con el cursor en esa línea, el texto muestra `[x]` o `[ ]`.
- [x] **Enlaces con Ctrl+clic**: un clic normal en un enlace solo pone el cursor. Con Ctrl+clic, «tareas» abre la nota, «La nota larga» abre [[edicion]], «edicion › Sección 10» lleva a esa sección y el enlace web abre la web.
- [x] **Enlace a una nota que no existe**: se ve gris y discontinuo. Con Ctrl+clic se crea y se abre en el editor.
- [x] **Tabla, fórmula y diagrama**: fuera de ellos se ven dibujados. Haz clic en la tabla: aparece su texto. Muévete con las flechas fuera de ella: vuelve a ser tabla. Haz lo mismo con la fórmula en bloque y con el diagrama.
- [x] **Imagen**: la onda se ve debajo de su línea.
- [x] **Tema claro y oscuro**: cambia de tema con el editor abierto y sin él. Los colores del editor deben seguir al tema.
- [x] **Ir y volver**: en [[edicion]], baja hasta la sección 10, pulsa Ctrl+E, escribe algo en la sección 12 y pulsa Ctrl+E. La vista debe quedar con ese párrafo a la misma altura.

## Si algo falla

Anota qué hiciste y qué pasó justo debajo de la casilla. Con eso basta para que lo corrija.
