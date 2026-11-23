{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, RankNTypes #-}
import Data.Kind (Type)
type T :: forall k. k -> Type
data T @_ @_ a = T
main :: IO ()
main = pure ()
