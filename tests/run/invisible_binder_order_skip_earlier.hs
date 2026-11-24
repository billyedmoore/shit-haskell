{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, RankNTypes #-}
import Data.Kind (Type)
type T :: forall {i} k. k -> forall j. j -> Type
data T a @j b = T
main :: IO ()
main = pure ()
