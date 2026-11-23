{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: a -> a
instance C Int where
  method :: Bool -> Bool
  method = id
main :: IO ()
main = pure ()
