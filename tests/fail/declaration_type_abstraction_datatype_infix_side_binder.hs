{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications,TypeOperators #-}
import Data.Kind (Type)
type (:+) :: forall k. k -> k -> Type
data a :+ b @k = T
main :: IO ()
main = pure ()
