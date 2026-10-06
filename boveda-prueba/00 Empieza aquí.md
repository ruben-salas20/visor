# Empieza aquí

Esta bóveda sirve para probar lo que no se puede comprobar de forma automática. Ve marcando cada casilla en el editor (Ctrl+E) a medida que pruebes.

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
- [ ] **Respuesta de un agente**: en una terminal ejecuta `visor notes`, copia un `id` y ejecuta `visor notes reply <id> Hola desde la terminal`. La respuesta debe aparecer en esa nota.
- [ ] **Huérfanas**: edita `pruebas/anotaciones.html` y borra una frase que hayas resaltado. Al guardar, esa anotación debe salir en la lista como «Huérfana».
- [ ] **Eliminar**: abre una anotación y pulsa «Eliminar».
- [ ] **Al cambiar el tamaño de la ventana**, los marcadores deben seguir en su sitio.

## Si algo falla

Anota qué hiciste y qué pasó justo debajo de la casilla. Con eso basta para que lo corrija.
