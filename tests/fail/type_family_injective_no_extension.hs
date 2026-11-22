{-# LANGUAGE TypeFamilies, NoTypeFamilyDependencies #-}

type family Result a = r | r -> a

main = pure ()
