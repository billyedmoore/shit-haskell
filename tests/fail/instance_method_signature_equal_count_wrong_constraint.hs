{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: Eq b => a -> b -> Bool
instance C Int where
  method :: Show b => Int -> b -> Bool
  method _ x = null (show x)
main :: IO ()
main = pure ()
