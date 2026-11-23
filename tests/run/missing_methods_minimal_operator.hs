{-# OPTIONS_GHC -Werror=missing-methods #-}

class Choose a where
  pick :: a -> a
  (.+.) :: a -> a -> a
  pick x = x .+. x
  x .+. _ = pick x
  {-# MINIMAL pick | (.+.) #-}

instance Choose Int where
  x .+. y = x + y

main :: IO ()
main = print (pick (2 :: Int))
