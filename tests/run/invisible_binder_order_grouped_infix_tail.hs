{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, TypeOperators, RankNTypes #-}
import Data.Kind (Type)
type (:+) :: Type -> Type -> forall k. Type
data (a :+ b) @k = MkT
main :: IO ()
main = pure ()
