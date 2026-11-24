{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, TypeOperators, RankNTypes #-}
import Data.Kind (Type)
type K = forall k. k -> Type
type T :: K
data T @k a = MkT
main :: IO ()
main = pure ()
