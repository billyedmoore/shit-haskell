{-# LANGUAGE TypeFamilyDependencies, NoTypeFamilies #-}

type family Result a = r | r -> a

main = pure ()
