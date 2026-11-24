{-# LANGUAGE ScopedTypeVariables #-}

import Data.Kind (Type)

class C a where
  f :: a -> Bool

instance forall (b :: Type). C Int where
  f _ = const @Bool @b True undefined

main :: IO ()
main = print (f (0 :: Int))
