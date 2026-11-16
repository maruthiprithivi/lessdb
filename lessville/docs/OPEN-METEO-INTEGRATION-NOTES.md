# Open-Meteo integration notes

Open-Meteo can provide current weather without an API key through `https://api.open-meteo.com/v1/forecast`. The official API documentation describes `latitude`, `longitude`, `current`, `timezone`, and current fields such as `temperature_2m`, `wind_speed_10m`, `precipitation`, `weather_code`, and `is_day`.

The free/open-access tier is intended for non-commercial use and is rate-limited to fewer than 10,000 calls/day, 5,000/hour and 600/minute. It has no uptime guarantee. Weather data requires attribution under CC BY 4.0. LessVille should therefore fetch at most once per hour (or less), cache the result in the world snapshot, show an Open-Meteo attribution link, and retain the deterministic fallback when the API is unavailable.

The public LessVille deployment is a research simulation and must not present the free endpoint as an uptime or production-weather guarantee. If the project becomes commercial, advert-supported, or exceeds the free limits, use a paid Open-Meteo plan or self-host the service instead of silently continuing on the free tier.

Sources:
- https://open-meteo.com/en/docs
- https://open-meteo.com/en/pricing
- https://open-meteo.com/en/terms
- https://open-meteo.com/en/licence
