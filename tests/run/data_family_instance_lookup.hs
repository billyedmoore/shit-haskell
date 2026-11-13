{-# LANGUAGE TypeFamilies, FlexibleInstances #-}
-- An instance for every application of a data family (`Describe (V a)`) serves a data
-- instance's type (`V Bool`), as a superclass of an instance at it (bitvec's
-- `instance Bits (Vector Bit)` needs vector's `Eq (Vector a)`).
module Main where

data family V a
data instance V Bool = VB Bool
newtype instance V Int = VI [Int]

class Describe a where
  describe :: a -> String

instance Describe (V a) where
  describe _ = "a V"

class Describe a => Named a where
  name :: a -> String

instance Named (V Bool) where
  name (VB b) = describe (VB b) ++ " of " ++ show b

instance Named (V Int) where
  name (VI xs) = describe (VI xs) ++ " of " ++ show (sum xs)

main :: IO ()
main = do
  putStrLn (name (VB True))
  putStrLn (name (VI [1, 2, 3]))
  putStrLn (describe (VI []))
