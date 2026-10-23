# Imagen para compilar la app de Android en ai-lab, sin instalar nada en el
# portátil. La usa compilar.sh; no es un servicio y no va en Compose.
FROM eclipse-temurin:21-jdk-noble

ARG CMDLINE_TOOLS=16111833
# ANDROID_USER_HOME vive en el volumen de la caché: así la clave de depuración
# es siempre la misma y el móvil acepta cada APK nuevo como actualización.
ENV ANDROID_HOME=/opt/android-sdk \
    GRADLE_USER_HOME=/gradle \
    ANDROID_USER_HOME=/gradle/android
ENV PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools

RUN apt-get update \
 && apt-get install -y --no-install-recommends unzip curl \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p $ANDROID_HOME/cmdline-tools \
 && curl -fsSL -o /tmp/tools.zip https://dl.google.com/android/repository/commandlinetools-linux-${CMDLINE_TOOLS}_latest.zip \
 && unzip -q /tmp/tools.zip -d $ANDROID_HOME/cmdline-tools \
 && mv $ANDROID_HOME/cmdline-tools/cmdline-tools $ANDROID_HOME/cmdline-tools/latest \
 && rm /tmp/tools.zip \
 && yes | sdkmanager --licenses > /dev/null \
 && sdkmanager "platforms;android-37.0" "build-tools;37.0.0" > /dev/null

# compilar.sh corre el contenedor con el usuario del host: el SDK debe
# admitir escrituras por si Gradle descarga algún componente que falte.
RUN chmod -R a+rwX $ANDROID_HOME && mkdir -p /gradle && chmod 777 /gradle

WORKDIR /src
