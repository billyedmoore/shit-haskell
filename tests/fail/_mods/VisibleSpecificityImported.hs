{-# LANGUAGE ExplicitForAll #-}
module VisibleSpecificityImported (inferred, pair) where
inferred x = x
pair :: forall {a} b. a -> b -> (a,b)
pair a b = (a,b)
