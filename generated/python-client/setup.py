from setuptools import setup, find_packages

setup(
    name="quorum-credit-client",
    version="1.0.0",
    description="Python client library for QuorumCredit API",
    author="QuorumCredit Contributors",
    author_email="info@quorumcredit.xyz",
    url="https://github.com/QuorumCredit/QuorumCredit",
    license="MIT",
    packages=find_packages(),
    install_requires=[
        "requests>=2.31.0",
    ],
    extras_require={
        "dev": [
            "pytest>=7.0.0",
            "black>=23.0.0",
            "mypy>=1.0.0",
        ],
    },
    python_requires=">=3.8",
    keywords=[
        "quorum",
        "credit",
        "api",
        "client",
        "stellar",
    ],
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Developers",
        "License :: OSI Approved :: MIT License",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.8",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
    ],
)
