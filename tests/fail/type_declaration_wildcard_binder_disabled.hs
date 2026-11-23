{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,NoTypeAbstractions #-}
import Data.Kind (Type)
data T _ a = T a
main :: IO ()
main = pure ()
