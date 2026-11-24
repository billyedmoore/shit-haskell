{-# LANGUAGE ExplicitForAll, DefaultSignatures, ConstrainedClassMethods #-}
class C a where
  m :: a -> b -> b
  default m :: forall b. Eq missing => a -> b -> b
  m _ = id
main :: IO ()
main = pure ()
