{-# LANGUAGE MultiWayIf #-}
main :: IO ()
main = print (if | 1 > (2 :: Int) -> "a" | otherwise -> "b")
