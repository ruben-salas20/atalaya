# Imagen para compilar la app sin instalar Java ni el SDK de Android en la
# máquina. La construye y la usa compilar.sh; no es un servicio.
# Mismas versiones que el flujo «Publicar versión»: JDK 21, SDK 37.
FROM eclipse-temurin:21-jdk-noble

ARG CMDLINE_TOOLS=16111833
ENV ANDROID_HOME=/opt/android-sdk
ENV PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools

RUN apt-get update \
 && apt-get install -y --no-install-recommends unzip curl \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p $ANDROID_HOME/cmdline-tools \
 && curl -fsSL -o /tmp/tools.zip https://dl.google.com/android/repository/commandlinetools-linux-${CMDLINE_TOOLS}_latest.zip \
 && unzip -q /tmp/tools.zip -d /tmp/tools \
 && mv /tmp/tools/cmdline-tools $ANDROID_HOME/cmdline-tools/latest \
 && rm -rf /tmp/tools /tmp/tools.zip \
 && yes | sdkmanager --licenses > /dev/null \
 && sdkmanager "platforms;android-37.0" "build-tools;37.0.0" > /dev/null

# El contenedor corre con el usuario del host. El SDK admite escrituras por si
# Gradle descarga un componente que falte, y /gradle nace abierto para que el
# volumen de la caché, al crearse vacío, herede esos permisos.
RUN chmod -R a+rwX $ANDROID_HOME && mkdir -p /gradle && chmod 777 /gradle

WORKDIR /src
