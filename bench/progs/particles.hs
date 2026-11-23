{-# LANGUAGE BangPatterns #-}
-- A small gravity simulation over strict records of Doubles: floating-point arithmetic,
-- records and list traversals.
data Body = Body { px, py, vx, vy, mass :: !Double }

step :: Double -> [Body] -> [Body]
step dt bodies = map move bodies
  where
    move b = let (ax, ay) = foldr (pull b) (0, 0) bodies
                 vx' = vx b + dt * ax
                 vy' = vy b + dt * ay
             in b { px = px b + dt * vx', py = py b + dt * vy', vx = vx', vy = vy' }
    pull b o (!ax, !ay)
      | d2 < 1e-9 = (ax, ay)
      | otherwise = (ax + f * dx, ay + f * dy)
      where dx = px o - px b
            dy = py o - py b
            d2 = dx * dx + dy * dy + 0.01
            f = mass o / (d2 * sqrt d2)

energy :: [Body] -> Double
energy bs = sum [0.5 * mass b * (vx b * vx b + vy b * vy b) | b <- bs]

main :: IO ()
main = do
  let bodies = [Body (fromIntegral i) (fromIntegral (i * i `mod` 7)) 0 0 (1 + fromIntegral (i `mod` 3)) | i <- [0 .. 39 :: Int]]
      final = iterate (step 0.001) bodies !! 2000
  print (round (energy final * 1000) :: Int)
