{-# LANGUAGE InstanceSigs, ScopedTypeVariables, ConstrainedClassMethods #-}
class C x where
  method :: Eq a => x -> a -> Bool
instance forall a. C [a] where
  method :: forall b. Show b => [a] -> b -> Bool
  method _ x = null (show x)

main :: IO ()
main = pure ()
