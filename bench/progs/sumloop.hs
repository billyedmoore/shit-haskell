{-# LANGUAGE BangPatterns #-}
-- A strict accumulating loop over Int.
loop :: Int -> Int -> Int
loop !acc i
  | i > 5000000 = acc
  | otherwise = loop (acc + (i * i) `mod` 7) (i + 1)

main :: IO ()
main = print (loop 0 1)
