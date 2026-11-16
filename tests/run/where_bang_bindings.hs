{-# LANGUAGE BangPatterns #-}
-- Banged where-bindings are evaluated before the right-hand side (and its guards) runs.
import Control.Exception
f :: Int -> Int
f n = n + 1 where !_unused = error "boom" :: Int
g :: Int -> Int
g n | n > 0 = 1 | otherwise = 2 where !_k = if n == 5 then error "five" else n
main = do
  r <- try (evaluate (f 1)); putStrLn (either (\(ErrorCall m) -> m) show r)
  print (g 3)
  r2 <- try (evaluate (g 5)); putStrLn (either (\(ErrorCall m) -> m) show r2)
